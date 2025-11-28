use async_channel::Sender;
use jiff::civil::Time;
use jiff::tz::TimeZone;
use sqlx::SqlitePool;
use std::{process, time::Instant};

use crate::C;
use crate::db::{ModelAlarm, ModelTimezone};
use crate::message_handler::{Msg, WSResponse};
use crate::request::PushRequest;
use crate::sysinfo::SysInfo;
use crate::ws_messages::{
    HourMinuteMsg, MessageValues, ParsedMessage, PiStatus, Response, TestRequest,
};
use crate::{app_env::AppEnv, ws_messages::to_struct};

const ONE_HOUR_AS_SEC: i64 = 60 * 60;

#[derive(Debug, Clone)]
pub struct WSSender {
    app_envs: AppEnv,
    connected_instant: Instant,
    sqlite: SqlitePool,
    tx: Sender<Msg>,
}

impl WSSender {
    pub fn new(app_envs: &AppEnv, sqlite: &SqlitePool, tx: &Sender<Msg>) -> Self {
        Self {
            app_envs: C!(app_envs),
            connected_instant: std::time::Instant::now(),
            sqlite: C!(sqlite),
            tx: C!(tx),
        }
    }
    /// Update the connected_instance time
    pub fn on_connection(&mut self) {
        self.connected_instant = std::time::Instant::now();
    }

    /// Handle text message, in this program they will all be json text
    pub async fn on_text(&self, message: String) {
        if let Some(data) = to_struct(&message) {
            match data {
                MessageValues::Invalid(error) => tracing::error!("invalid::{error:?}"),
                MessageValues::Valid(msg, unique) => match msg {
                    ParsedMessage::AlarmAdd(hm) => self.alarm_add(hm).await,
                    ParsedMessage::AlarmDelete => self.alarm_delete(unique).await,
                    ParsedMessage::AlarmDismiss => self.alarm_dismiss().await,
                    ParsedMessage::AlarmUpdate(hm) => self.alarm_update(hm, unique).await,
                    ParsedMessage::Restart => self.restart().await,
                    ParsedMessage::Status => self.send_status().await,
                    ParsedMessage::TestRequest(msg) => self.test_request(msg).await,
                    ParsedMessage::TimeZone(timezone) => {
                        self.time_zone(timezone.zone, unique).await;
                    }
                },
            }
        }
    }

    /// Generate, and send, pi information
    pub async fn send_status(&self) {
        let (info, alarms) = tokio::join!(
            SysInfo::new(&self.sqlite, &self.app_envs),
            ModelAlarm::get(&self.sqlite)
        );

        let info = PiStatus::new(
            info,
            alarms.unwrap_or_default(),
            self.connected_instant.elapsed().as_secs(),
        );
        self.send_ws_response(WSResponse {
            response: Response::Status(info),
            cache: Some(true),
            unique: None,
        })
        .await;
    }

    /// Send a message to close the socket
    async fn close(&self) {
        self.tx.send(Msg::WsClose).await.ok();
    }

    /// Send a unique error message
    async fn send_error(&self, message: &str, unique: Option<String>) {
        self.send_ws_response(WSResponse {
            response: Response::Error(message.to_owned()),
            cache: None,
            unique,
        })
        .await;
    }

    async fn send_ws_response(&self, response: WSResponse) {
        match self.tx.send(Msg::ToSend(response)).await {
            Ok(()) => (),
            Err(e) => {
                tracing::error!("{e}");
            }
        }
    }

    /// Restart alarm loop, and send status to client
    async fn reset_alarm_send_status(&self) {
        _ = tokio::join!(self.tx.send(Msg::AlarmLoopReset), self.send_status());
    }

    async fn too_close(&self, unique: String) {
        self.send_error("Current time too close to alarm to edit", Some(unique))
            .await;
    }

    /// Validate that an alarm can be edited, need to be more than six hour difference
    fn valid_change(current_time: Time, alarm_hour: i8, alarm_minute: i8) -> Result<(), ()> {
        let current_as_sec =
            i64::from(current_time.hour()) * 60 * 60 + i64::from(current_time.minute()) * 60;

        let alarm_as_sec = i64::from(alarm_hour) * 60 * 60 + i64::from(alarm_minute) * 60;

        // alarm is in range 0-5
        if alarm_as_sec < ONE_HOUR_AS_SEC * 5 {
            let limit = ONE_HOUR_AS_SEC * 24 + (alarm_as_sec - ONE_HOUR_AS_SEC * 5);
            if current_as_sec >= limit {
                return Err(());
            }
        }

        // alarm is > 23
        if alarm_as_sec > ONE_HOUR_AS_SEC * 23 {
            let limit = (alarm_as_sec + ONE_HOUR_AS_SEC) - ONE_HOUR_AS_SEC * 24;
            if current_as_sec <= limit {
                return Err(());
            }
        }

        let lower_range = alarm_as_sec - ONE_HOUR_AS_SEC * 5;

        if (lower_range..=alarm_as_sec).contains(&current_as_sec) {
            return Err(());
        }

        Ok(())
    }

    /// Add a new alarm to database, and update alarm_schedule
    async fn alarm_add(&self, hm: HourMinuteMsg) {
        if let Err(e) = ModelAlarm::add(&self.sqlite, hm).await {
            tracing::error!("{e}");
        } else {
            self.reset_alarm_send_status().await;
        }
    }

    /// Delete all alarm in database, and update alarm_schedule
    async fn alarm_delete(&self, unique: String) {
        if let Ok(Some(alarm)) = ModelAlarm::get(&self.sqlite).await
            && let Some(current_time) = ModelTimezone::get(&self.sqlite).await
        {
            let current_time = current_time.to_time();
            if Self::valid_change(current_time, alarm.hour, alarm.minute).is_ok() {
                if let Err(e) = ModelAlarm::delete(&self.sqlite).await {
                    tracing::error!("{e}");
                }
                self.reset_alarm_send_status().await;
            } else {
                self.too_close(unique).await;
            }
        }
    }
    /// Add a new alarm to database, and update alarm_schedule
    async fn alarm_dismiss(&self) {
        self.tx.send(Msg::AlarmDissmiss).await.ok();
    }

    /// Update the alarm in the database, and update alarm_schedule
    async fn alarm_update(&self, hm: HourMinuteMsg, unique: String) {
        if let Ok(Some(alarm)) = ModelAlarm::get(&self.sqlite).await
            && let Some(current_time) = ModelTimezone::get(&self.sqlite).await
        {
            let current_time = current_time.to_time();
            if Self::valid_change(current_time, alarm.hour, alarm.minute).is_ok() {
                if let Err(e) = ModelAlarm::update(&self.sqlite, hm).await {
                    tracing::error!("{e}");
                }
                self.reset_alarm_send_status().await;
            } else {
                self.too_close(unique).await;
            }
        }
    }

    /// Force quite program, assumes running in an auto-restart container, or systemd, in order to start again immediately
    async fn restart(&self) {
        self.close().await;
        process::exit(0);
    }

    /// Send a test request of a given message
    async fn test_request(&self, msg: TestRequest) {
        if let Err(e) = PushRequest::TestRequest
            .make_request(&self.app_envs, &self.sqlite, &msg.message)
            .await
        {
            tracing::error!("{e}");
        }
    }

    /// Change the timezone in database to new given database,
    /// also update timezone in alarm scheduler
    async fn time_zone(&self, zone: String, unique: String) {
        if let Some(alarm) = ModelAlarm::get(&self.sqlite).await.unwrap_or_default()
            && let Some(current_time) = ModelTimezone::get(&self.sqlite).await
            && Self::valid_change(current_time.to_time(), alarm.hour, alarm.minute).is_err()
        {
            self.too_close(unique).await;
            return;
        }

        if TimeZone::get(&zone).is_ok() {
            ModelTimezone::update(&self.sqlite, &zone).await.ok();
            self.reset_alarm_send_status().await;
        } else {
            self.send_error("Invalid timezone", Some(unique)).await;
        }
    }
}

/// ws_sender
///
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ws_sender_valid_change() {
        let test = |alarm: (i8, i8), current_time: (i8, i8), is_ok: bool| {
            let result = WSSender::valid_change(
                jiff::civil::time(current_time.0, current_time.1, 0, 0),
                alarm.0,
                alarm.1,
            );
            if is_ok {
                assert!(result.is_ok());
            } else {
                assert!(result.is_err());
            }
        };

        // 5:15 am alarm
        let alarm = (5, 15);
        test(alarm, (23, 59), true);
        test(alarm, (0, 10), true);
        test(alarm, (0, 15), false);
        test(alarm, (1, 59), false);
        test(alarm, (5, 0), false);
        test(alarm, (5, 14), false);
        test(alarm, (5, 15), false);
        test(alarm, (5, 16), true);

        // 06:15 alarm
        let alarm = (6, 15);
        test(alarm, (23, 59), true);
        test(alarm, (0, 0), true);
        test(alarm, (0, 15), true);
        test(alarm, (1, 14), true);
        test(alarm, (1, 15), false);
        test(alarm, (3, 59), false);
        test(alarm, (6, 14), false);
        test(alarm, (6, 15), false);
        test(alarm, (6, 16), true);

        // 12:15 alarm
        let alarm = (12, 15);
        test(alarm, (6, 59), true);
        test(alarm, (7, 0), true);
        test(alarm, (12, 14), false);
        test(alarm, (12, 15), false);
        test(alarm, (12, 16), true);
        test(alarm, (14, 0), true);

        // 18:15 alarm
        let alarm = (18, 15);
        test(alarm, (1, 15), true);
        test(alarm, (12, 59), true);
        test(alarm, (13, 14), true);
        test(alarm, (13, 15), false);
        test(alarm, (18, 14), false);
        test(alarm, (18, 15), false);
        test(alarm, (18, 16), true);
        test(alarm, (23, 15), true);

        // 00:15 alarm
        let alarm = (0, 15);
        test(alarm, (16, 14), true);
        test(alarm, (19, 14), true);
        test(alarm, (19, 15), false);
        test(alarm, (23, 14), false);
        test(alarm, (0, 14), false);
        test(alarm, (0, 15), false);
        test(alarm, (0, 16), true);
    }
}

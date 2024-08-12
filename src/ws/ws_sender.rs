use futures_util::lock::Mutex;
use futures_util::SinkExt;
use sqlx::SqlitePool;
use std::{process, sync::Arc, time::Instant};
use time::Time;
use time_tz::timezones;
use tokio::sync::mpsc::Sender;
use tracing::{error, trace};

use crate::alarm_schedule::CronMessage;
use crate::request::PushRequest;
use crate::sysinfo::SysInfo;
use crate::ws_messages::{
    MessageValues, ParsedMessage, PiStatus, Response, StructuredResponse, TestRequest,
};
use crate::{
    app_env::AppEnv,
    db::{ModelAlarm, ModelTimezone},
    ws_messages::to_struct,
};

const ONE_HOUR_AS_SEC: i64 = 60 * 60;

use super::WSWriter;

#[derive(Debug, Clone)]
pub struct WSSender {
    app_envs: AppEnv,
    connected_instant: Instant,
    db: SqlitePool,
    sx: Sender<CronMessage>,
    writer: Arc<Mutex<WSWriter>>,
    unique: Option<String>,
}

impl WSSender {
    pub fn new(
        app_envs: &AppEnv,
        connected_instant: Instant,
        db: &SqlitePool,
        sx: Sender<CronMessage>,
        writer: Arc<Mutex<WSWriter>>,
    ) -> Self {
        Self {
            app_envs: app_envs.clone(),
            connected_instant,
            db: db.clone(),
            sx,
            writer,
            unique: None,
        }
    }

    /// Handle text message, in this program they will all be json text
    pub async fn on_text(&mut self, message: String) {
        if let Some(data) = to_struct(&message) {
            match data {
                MessageValues::Invalid(error) => error!("invalid::{error:?}"),
                MessageValues::Valid(msg, unique) => {
                    self.unique = Some(unique);
                    match msg {
                        ParsedMessage::AlarmDelete => self.alarm_delete().await,
                        ParsedMessage::AlarmDismiss => self.alarm_dismiss().await,

                        ParsedMessage::AlarmUpdate(hm) => {
                            self.alarm_update(hm.hour, hm.minute).await;
                        }
                        ParsedMessage::AlarmAdd(hm) => {
                            self.alarm_add(hm.hour, hm.minute).await;
                        }
                        ParsedMessage::Restart => self.restart().await,
                        ParsedMessage::TestRequest(msg) => self.test_request(msg).await,
                        ParsedMessage::TimeZone(timezone) => self.time_zone(timezone.zone).await,
                        ParsedMessage::Status => self.send_status().await,
                    }
                }
            }
        }
    }

    /// Send a test request of a given message
    async fn test_request(&mut self, msg: TestRequest) {
        if let Err(e) = PushRequest::Test(msg.message)
            .make_request(&self.app_envs, &self.db)
            .await
        {
            tracing::error!("{e}");
        }
    }

    // TODO check this when changing timezone? Use current time + current alarm (if set)
    /// Validate that an alarm can be edited, need to be more than six hour difference
    fn valid_change(current_time: Time, alarm_hour: u8, alarm_minute: u8) -> Result<(), ()> {
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
    async fn alarm_add(&mut self, hour: u8, minute: u8) {
        if let Err(e) = ModelAlarm::add(&self.db, (hour, minute)).await {
            tracing::error!("{e}");
            self.send_error(&format!("{e}")).await;
        } else {
            self.sx.send(CronMessage::Reset).await.ok();
            self.send_status().await;
        }
    }

    /// Add a new alarm to database, and update alarm_schedule
    async fn alarm_dismiss(&mut self) {
        self.sx.send(CronMessage::AlarmDismiss).await.ok();
    }

    /// Delete all alarm in database, and update alarm_schedule
    async fn alarm_delete(&mut self) {
        if let Ok(Some(alarm)) = ModelAlarm::get(&self.db).await {
            if let Some(current_time) = ModelTimezone::get(&self.db)
                .await
                .unwrap_or_default()
                .to_time()
            {
                if Self::valid_change(current_time, alarm.hour, alarm.minute).is_ok() {
                    if let Err(e) = ModelAlarm::delete(&self.db).await {
                        tracing::error!("{e}");
                    }
                    self.sx.send(CronMessage::Reset).await.ok();
                    self.send_status().await;
                } else {
                    self.too_close().await;
                }
            }
        }
    }

    /// Update the alarm in the database, and update alarm_schedule
    async fn alarm_update(&mut self, hour: u8, minute: u8) {
        if let Ok(Some(alarm)) = ModelAlarm::get(&self.db).await {
            if let Some(current_time) = ModelTimezone::get(&self.db)
                .await
                .unwrap_or_default()
                .to_time()
            {
                if Self::valid_change(current_time, alarm.hour, alarm.minute).is_ok() {
                    if let Err(e) = ModelAlarm::update(&self.db, (hour, minute)).await {
                        tracing::error!("{e}");
                    }
                    self.sx.send(CronMessage::Reset).await.ok();
                    self.send_status().await;
                } else {
                    self.too_close().await;
                }
            }
        }
    }

    async fn too_close(&mut self) {
        self.send_error("Current time too close to alarm to edit")
            .await;
    }

    /// Force quite program, assumes running in an auto-restart container, or systemd, in order to start again immediately
    async fn restart(&mut self) {
        self.close().await;
        process::exit(0);
    }

    /// Change the timezone in database to new given database,
    /// also update timezone in alarm scheduler
    async fn time_zone(&mut self, zone: String) {
        if timezones::get_by_name(&zone).is_some() {
            ModelTimezone::update(&self.db, &zone).await.ok();
            self.sx.send(CronMessage::Reset).await.ok();
            self.send_status().await;
        } else {
            self.send_error("Invalid timezone").await;
        }
    }

    /// Send a message to the socket
    /// cache could just be Option<()>, and if some then send true?
    async fn send_ws_response(
        &mut self,
        response: Response,
        cache: Option<bool>,
        unique: Option<String>,
    ) {
        match self
            .writer
            .lock()
            .await
            .send(StructuredResponse::data(response, cache, unique))
            .await
        {
            Ok(()) => trace!("Message sent"),
            Err(e) => {
                error!("send_ws_response::SEND-ERROR::{e:?}");
                process::exit(1);
            }
        }
    }

    /// Send a unique error message
    pub async fn send_error(&mut self, message: &str) {
        self.send_ws_response(
            Response::Error(message.to_owned()),
            None,
            self.unique.clone(),
        )
        .await;
    }

    /// Generate, and send, pi information
    pub async fn send_status(&mut self) {
        let info = SysInfo::new(&self.db, &self.app_envs).await;
        let alarms = ModelAlarm::get(&self.db).await.unwrap_or_default();
        let info = PiStatus::new(info, alarms, self.connected_instant.elapsed().as_secs());
        self.send_ws_response(Response::Status(info), Some(true), None)
            .await;
    }

    /// close connection, uses a 2 second timeout
    pub async fn close(&mut self) {
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            self.writer.lock().await.close(),
        )
        .await
        .ok()
        .map(std::result::Result::ok);
    }
}

/// ws_sender
///
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::too_many_lines)]
mod tests {
    use super::*;

    #[test]
    fn test_ws_sender_valid_change() {
        let test = |alarm: (u8, u8), current_time: (u8, u8), is_ok: bool| {
            let result = WSSender::valid_change(
                Time::from_hms(current_time.0, current_time.1, 0).unwrap(),
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

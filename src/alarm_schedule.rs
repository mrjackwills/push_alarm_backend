use std::time::Duration;

use sqlx::SqlitePool;
use tokio::{
    sync::mpsc::{Receiver, Sender},
    task::JoinHandle,
};

use crate::{
    C,
    app_env::AppEnv,
    app_error::AppError,
    db::{ModelAlarm, ModelObliqueStrategy, ModelTimezone},
    request::PushRequest,
};

const ONE_SEC: u64 = 1000;
const TWENTY_FIVE_SEC: Duration = std::time::Duration::from_secs(25);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum CronMessage {
    Reset,
    AlarmStart(Option<String>),
    AlarmDismiss,
}

#[derive(Debug)]
pub struct AlarmSchedule {
    app_env: AppEnv,
    loop_alarm: Option<JoinHandle<()>>,
    loop_msg: Option<JoinHandle<()>>,
    rx: Receiver<CronMessage>,
    sqlite: SqlitePool,
    sx: Sender<CronMessage>,
    time_zone: ModelTimezone,
}

impl AlarmSchedule {
    pub async fn init(
        sqlite: SqlitePool,
        app_env: AppEnv,
    ) -> Result<Sender<CronMessage>, AppError> {
        let time_zone = ModelTimezone::get(&sqlite).await.unwrap_or_default();
        let (sx, rx) = tokio::sync::mpsc::channel(128);

        let mut alarm_schedule = Self {
            app_env: C!(app_env),
            loop_alarm: None,
            loop_msg: None,
            rx,
            sqlite,
            sx: C!(sx),
            time_zone,
        };
        alarm_schedule.generate_alarm_loop().await?;
        tokio::spawn(async move {
            alarm_schedule.message_looper().await;
        });

        Ok(sx)
    }

    /// TODO test me
    async fn get_message(sqlite: &SqlitePool, msg: Option<String>) -> String {
        if let Some(msg) = msg
            && !msg.is_empty()
        {
            msg
        } else {
            ModelObliqueStrategy::get_random(sqlite)
                .await
                .unwrap_or_else(|_| String::from("fix me"))
        }
    }

    async fn message_looper(&mut self) {
        while let Some(x) = self.rx.recv().await {
            match x {
                CronMessage::Reset => {
                    if let Some(looper) = self.loop_msg.as_ref() {
                        looper.abort();
                    }
                    self.refresh_timezone().await;
                    if let Err(e) = self.generate_alarm_loop().await {
                        println!("Can't generate new alarm loop");
                        println!("{e}");
                    }
                }
                CronMessage::AlarmDismiss => {
                    if let Some(looper) = self.loop_alarm.as_ref() {
                        looper.abort();
                    }
                }
                CronMessage::AlarmStart(msg) => {
                    let sqlite = C!(self.sqlite);
                    let app_envs = C!(self.app_env);
                    let msg = Self::get_message(&self.sqlite, msg).await;
                    self.loop_alarm = Some(tokio::spawn(async move {
                        for i in 1..=40 {
                            if let Err(e) = PushRequest::Alarm(i)
                                .make_request(&app_envs, &sqlite, &msg)
                                .await
                            {
                                tracing::error!("{e}");
                            }
                            tokio::time::sleep(TWENTY_FIVE_SEC).await;
                        }
                    }));
                }
            }
        }
    }

    async fn generate_alarm_loop(&mut self) -> Result<(), AppError> {
        if let Some(alarm) = ModelAlarm::get(&self.sqlite).await? {
            let tz = C!(self.time_zone);
            let sx = C!(self.sx);
            self.loop_msg = Some(tokio::spawn(async move {
                Self::init_alarm_loop(alarm, tz, sx).await;
            }));
        }
        Ok(())
    }

    // Get timezone from db and store into self, also update offset
    async fn refresh_timezone(&mut self) {
        if let Some(time_zone) = ModelTimezone::get(&self.sqlite).await
            && self.time_zone != time_zone
        {
            self.time_zone = time_zone;
        }
    }

    /// loop every 1 second,check if current time & day matches alarm, and if so execute alarm illuminate
    /// is private, so that it can only be executed during the self.init() method, so that it is correctly spawned onto it's own tokio thread
    async fn init_alarm_loop(alarm: ModelAlarm, time_zone: ModelTimezone, sx: Sender<CronMessage>) {
        loop {
            let start = std::time::Instant::now();
            let current_time = time_zone.to_time();
            if alarm.hour == current_time.hour()
                && alarm.minute == current_time.minute()
                && current_time.second() == 0
            {
                sx.send(CronMessage::AlarmStart(alarm.message.clone()))
                    .await
                    .ok();
            }
            let to_sleep = ONE_SEC
                .saturating_sub(u64::try_from(start.elapsed().as_millis()).unwrap_or(ONE_SEC));
            tokio::time::sleep(std::time::Duration::from_millis(to_sleep)).await;
        }
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used)]
mod tests {

    use std::collections::HashSet;

    use crate::tests::{test_cleanup, test_setup};

    use super::*;

    #[tokio::test]
    async fn model_oblique_trategy_seed() {
        let (_, sqlite, uuid) = test_setup().await;
        ModelObliqueStrategy::seed_stratergies(&sqlite)
            .await
            .unwrap();

        let all_stratergies = include_str!("../src/db/oblique.txt");
        let mut set = HashSet::new();
        for i in all_stratergies.lines() {
            set.insert(i.to_owned());
        }

        let result = AlarmSchedule::get_message(&sqlite, Some("custom".to_owned())).await;
        assert_eq!(result, "custom");

        let result = AlarmSchedule::get_message(&sqlite, Some(String::new())).await;
        assert!(set.contains(&result));

        let result = AlarmSchedule::get_message(&sqlite, None).await;
        assert!(set.contains(&result));
        test_cleanup(uuid, Some(sqlite)).await;
    }
}

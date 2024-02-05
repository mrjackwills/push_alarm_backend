use std::time::Duration;

use sqlx::SqlitePool;
use tokio::{
    sync::mpsc::{Receiver, Sender},
    task::JoinHandle,
};

use crate::{
    app_env::AppEnv,
    app_error::AppError,
    db::{ModelAlarm, ModelTimezone},
    request::PushRequest,
};

const ONE_SECOND: u64 = 1000;
const FORTY_FIVE_SEC: Duration = std::time::Duration::from_secs(45);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CronMessage {
    Reset,
    Alarm,
}

#[derive(Debug)]
pub struct AlarmSchedule {
    app_env: AppEnv,
    looper: Option<JoinHandle<()>>,
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
            app_env: app_env.clone(),
            looper: None,
            rx,
            sqlite,
            sx: sx.clone(),
            time_zone,
        };
        alarm_schedule.generate_alarm_loop().await?;
        tokio::spawn(async move {
            alarm_schedule.message_looper().await;
        });

        Ok(sx)
    }

    async fn message_looper(&mut self) {
        while let Some(x) = self.rx.recv().await {
            match x {
                CronMessage::Reset => {
                    if let Some(looper) = self.looper.as_ref() {
                        looper.abort();
                    }
                    self.refresh_timezone().await;
                    if let Err(e) = self.generate_alarm_loop().await {
                        println!("Can't generate new alarm loop");
                        println!("{e}");
                    }
                }
                CronMessage::Alarm => {
                    let sqlite = self.sqlite.clone();
                    let app_envs = self.app_env.clone();
                    tokio::spawn(async move {
                        for i in 1..=8 {
                            if let Err(e) =
                                PushRequest::Alarm(i).make_request(&app_envs, &sqlite).await
                            {
                                tracing::error!("{e}");
                            }
                            tokio::time::sleep(FORTY_FIVE_SEC).await;
                        }
                    });
                }
            }
        }
    }

    async fn generate_alarm_loop(&mut self) -> Result<(), AppError> {
        if let Some(alarm) = ModelAlarm::get(&self.sqlite).await? {
            let tz = self.time_zone.clone();
            let sx = self.sx.clone();
            self.looper = Some(tokio::spawn(async move {
                Self::init_alarm_loop(alarm, tz, sx).await;
            }));
        }
        Ok(())
    }

    // Get timezone from db and store into self, also update offset
    async fn refresh_timezone(&mut self) {
        if let Some(time_zone) = ModelTimezone::get(&self.sqlite).await {
            if self.time_zone != time_zone {
                self.time_zone = time_zone;
            }
        }
    }

    /// loop every 1 second,check if current time & day matches alarm, and if so execute alarm illuminate
    /// is private, so that it can only be executed during the self.init() method, so that it is correctly spawned onto it's own tokio thread
    async fn init_alarm_loop(alarm: ModelAlarm, time_zone: ModelTimezone, sx: Sender<CronMessage>) {
        loop {
            let start = std::time::Instant::now();
            if let Some(current_time) = time_zone.to_time() {
                if alarm.hour == current_time.hour()
                    && alarm.minute == current_time.minute()
                    && current_time.second() == 0
                {
                    sx.send(CronMessage::Alarm).await.ok();
                }
            }
            let to_sleep = ONE_SECOND
                .saturating_sub(u64::try_from(start.elapsed().as_millis()).unwrap_or(ONE_SECOND));
            tokio::time::sleep(std::time::Duration::from_millis(to_sleep)).await;
        }
    }
}

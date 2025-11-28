use async_channel::Sender;
use jiff::civil::Time;
use sqlx::SqlitePool;
use tokio_util::sync::CancellationToken;

use crate::{
    C,
    db::{ModelAlarm, ModelTimezone},
    message_handler::Msg,
    sleep,
};

#[derive(Debug)]
pub struct AlarmSchedule {
    tx: Sender<Msg>,
    token: Option<CancellationToken>,
}

impl AlarmSchedule {
    pub fn new(tx: &Sender<Msg>) -> Self {
        Self {
            tx: C!(tx),
            token: None,
        }
    }

    /// Cancel the current token, set a new one, and return it
    fn get_set_cancel_token(&mut self) -> CancellationToken {
        if let Some(token) = &self.token {
            token.cancel();
        }
        let token = CancellationToken::new();
        self.token = Some(C!(token));
        token
    }

    // How to cancel this
    /// Start the alarm looper thread
    pub async fn start_alarm_thread(&mut self, sqlite: &SqlitePool) {
        let (tz, alarm) = tokio::join!(ModelTimezone::get(sqlite), ModelAlarm::get(sqlite));
        let token = self.get_set_cancel_token();

        if let Ok(Some(alarm)) = alarm {
            let tx = C!(self.tx);
            tokio::spawn(async move {
                token
                    .run_until_cancelled(Self::init_alarm(alarm, tz.unwrap_or_default(), tx))
                    .await
            });
        }
    }

    /// Work out how long need to sleep for
    fn generate_sleep_time(alarm: &ModelAlarm, time_zone: &ModelTimezone) -> u64 {
        let current_time = time_zone.to_time();
        let ms = current_time
            .duration_until(Time::new(alarm.hour, alarm.minute, 0, 0).unwrap_or_default())
            .as_millis();
        u64::try_from(if ms < 0 { 60 * 60 * 24 * 1000 + ms } else { ms }).unwrap_or_default()
    }

    async fn init_alarm(alarm: ModelAlarm, time_zone: ModelTimezone, tx: Sender<Msg>) {
        loop {
            let sleep_for = Self::generate_sleep_time(&alarm, &time_zone);
            if sleep_for > 0 {
                sleep!(sleep_for);
                tx.send(Msg::AlarmStart(C!(alarm.message))).await.ok();
            } else {
                sleep!(1000);
            }
        }
    }
}

use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

mod alarm_schedule;
mod app_env;
mod app_error;
mod db;
mod request;
mod sysinfo;
mod word_art;
mod ws;
mod ws_messages;

use alarm_schedule::AlarmSchedule;
use app_env::AppEnv;
use app_error::AppError;
use db::init_db;
use word_art::Intro;
use ws::open_connection;

use crate::db::ModelObliqueStrategy;

/// Simple macro to create a new String, or convert from a &str to  a String - basically just gets rid of String::from() / .to_owned() etc
#[macro_export]
macro_rules! S {
    () => {
        String::new()
    };
    ($s:expr) => {
        String::from($s)
    };
}

/// Simple macro to call `.clone()` on whatever is passed in
#[macro_export]
macro_rules! C {
    ($i:expr) => {
        $i.clone()
    };
}

fn close_signal() {
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        std::process::exit(1);
    });
}

fn setup_tracing(app_envs: &AppEnv) {
    tracing_subscriber::fmt()
        .with_max_level(app_envs.log_level)
        .init();
}

async fn start() -> Result<(), AppError> {
    let app_envs = AppEnv::get();
    setup_tracing(&app_envs);
    Intro::new(&app_envs).show();
    let sqlite = init_db(&app_envs).await?;
    ModelObliqueStrategy::seed_stratergies(&sqlite).await?;
    close_signal();
    let sx = AlarmSchedule::init(C!(sqlite), C!(app_envs)).await?;
    open_connection(app_envs, sqlite, sx).await?;
    Ok(())
}
#[tokio::main]
async fn main() -> Result<(), AppError> {
    tokio::spawn(async move {
        if let Err(e) = start().await {
            tracing::error!("{e:?}");
        }
    })
    .await
    .ok();
    Ok(())
}

#[cfg(test)]
#[expect(clippy::unwrap_used)]
mod tests {
    use std::{path::PathBuf, time::SystemTime};

    use jiff::tz::TimeZone;
    use sqlx::SqlitePool;
    use uuid::Uuid;

    use crate::{app_env::AppEnv, db::init_db};
    /// Close database connection, and delete all test files
    pub async fn test_cleanup(uuid: Uuid, sqlite: Option<SqlitePool>) {
        if let Some(sqlite) = sqlite {
            sqlite.close().await;
        }
        let sql_name = PathBuf::from(format!("/dev/shm/{uuid}.db"));
        let sql_sham = sql_name.join("-shm");
        let sql_wal = sql_name.join("-wal");
        tokio::fs::remove_file(sql_wal).await.ok();
        tokio::fs::remove_file(sql_sham).await.ok();
        tokio::fs::remove_file(sql_name).await.ok();
    }

    /// The uuid is used as a file location for sqlite, at /dev/shm/{ uuid }.db
    pub fn gen_app_envs(uuid: Uuid) -> AppEnv {
        AppEnv {
            location_sqlite: format!("/dev/shm/{uuid}.db"),
            log_level: tracing::Level::INFO,
            start_time: SystemTime::now(),
            timezone: TimeZone::get("Europe/London").unwrap(),
            token_app: S!("test_token_app"),
            token_user: S!("test_token_user"),
            ws_address: S!("ws_address"),
            ws_apikey: S!("ws_apikey"),
            ws_password: S!("ws_password"),
            ws_token_address: S!("ws_token_address"),
        }
    }

    pub async fn test_setup() -> (AppEnv, SqlitePool, Uuid) {
        let uuid = Uuid::new_v4();
        let app_envs = gen_app_envs(uuid);
        let sqlite = init_db(&app_envs).await.unwrap();
        (app_envs, sqlite, uuid)
    }

    #[macro_export]
    /// Sleep for a given number of milliseconds, is an async fn.
    /// If no parameter supplied, defaults to 1000ms
    macro_rules! sleep {
        () => {
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        };
        ($ms:expr) => {
            tokio::time::sleep(std::time::Duration::from_millis($ms)).await;
        };
    }
}

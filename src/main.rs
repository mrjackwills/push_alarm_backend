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

#[tokio::main]
async fn main() -> Result<(), AppError> {
    let app_envs = AppEnv::get();
    setup_tracing(&app_envs);
    Intro::new(&app_envs).show();

    let sqlite = init_db(&app_envs).await?;
    close_signal();
    let sx = AlarmSchedule::init(sqlite.clone(), app_envs.clone()).await?;
    open_connection(app_envs, sqlite, sx).await?;
    Ok(())
}

#[cfg(test)]
#[expect(clippy::unwrap_used)]
mod tests {
    use std::{path::PathBuf, time::SystemTime};

    use sqlx::SqlitePool;
    use uuid::Uuid;

    use crate::{
        app_env::{AppEnv, EnvTimeZone},
        db::init_db,
    };
    /// Close database connection, and delete all test files
    pub async fn test_cleanup(uuid: Uuid, db: Option<SqlitePool>) {
        if let Some(db) = db {
            db.close().await;
        }
        let sql_name = PathBuf::from(format!("/dev/shm/{uuid}.db"));
        let sql_sham = sql_name.join("-shm");
        let sql_wal = sql_name.join("-wal");
        tokio::fs::remove_file(sql_wal).await.ok();
        tokio::fs::remove_file(sql_sham).await.ok();
        tokio::fs::remove_file(sql_name).await.ok();
    }

    pub fn gen_app_envs(uuid: Uuid) -> AppEnv {
        AppEnv {
            location_sqlite: format!("/dev/shm/{uuid}.db"),
            log_level: tracing::Level::INFO,
            start_time: SystemTime::now(),
            timezone: EnvTimeZone::new("Europe/London"),
            token_app: String::from("test_token_app"),
            token_user: "test_token_user".to_owned(),
            ws_address: "ws_address".to_owned(),
            ws_apikey: "ws_apikey".to_owned(),
            ws_password: "ws_password".to_owned(),
            ws_token_address: "ws_token_address".to_owned(),
        }
    }

    pub async fn test_setup() -> (AppEnv, SqlitePool, Uuid) {
        let uuid = Uuid::new_v4();
        let app_envs = gen_app_envs(uuid);
        let db = init_db(&app_envs).await.unwrap();
        (app_envs, db, uuid)
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

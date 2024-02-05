use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::fs::read_to_string;

use crate::{app_env::AppEnv, db::ModelTimezone};

#[derive(Debug, Serialize, Deserialize)]
pub struct SysInfo {
    pub uptime: usize,
    pub version: String,
    pub uptime_app: u64,
    pub time_zone: String,
}

impl SysInfo {
    /// Get uptime by reading, and parsing, /proc/uptime file
    async fn get_uptime() -> usize {
        let uptime = read_to_string("/proc/uptime").await.unwrap_or_default();
        let (uptime, _) = uptime.split_once('.').unwrap_or_default();
        uptime.parse::<usize>().unwrap_or_default()
    }

    /// Generate sysinfo struct, will valid data
    pub async fn new(db: &SqlitePool, app_envs: &AppEnv) -> Self {
        let model_timezone = ModelTimezone::get(db).await.unwrap_or_default();
        Self {
            uptime: Self::get_uptime().await,
            uptime_app: std::time::SystemTime::now()
                .duration_since(app_envs.start_time)
                .map_or(0, |value| value.as_secs()),
            time_zone: model_timezone.zone_name,
            version: env!("CARGO_PKG_VERSION").into(),
        }
    }
}

// SysInfo tests
//
/// cargo watch -q -c -w src/ -x 'test sysinfo -- --test-threads=1 --nocapture'
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::tests::{test_cleanup, test_setup};

    use super::*;

    #[tokio::test]
    async fn sysinfo_getuptime_ok() {
        let (_, db, uuid) = test_setup().await;

        let result = SysInfo::get_uptime().await;

        // Assumes ones computer has been turned on for one minute
        assert!(result > 60);
        test_cleanup(uuid, Some(db)).await;
    }

    #[tokio::test]
    async fn sysinfo_get_sysinfo_ok() {
        let (app_envs, db, uuid) = test_setup().await;
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        let result = SysInfo::new(&db, &app_envs).await;

        assert_eq!(result.time_zone, "Europe/London");
        assert_eq!(result.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(result.uptime_app, 1);
        // Again assume ones computer has been turned on for one minute
        assert!(result.uptime > 60);
        test_cleanup(uuid, Some(db)).await;
    }
}

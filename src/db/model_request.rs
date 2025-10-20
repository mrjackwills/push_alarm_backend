use jiff::{SpanRound, ToSpan, Unit};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::{fmt, time::SystemTime};

use crate::app_error::AppError;
use crate::request::PushRequest;

#[derive(sqlx::FromRow, Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ModelRequest {
    pub request_id: i64,
    #[sqlx(try_from = "i64")]
    pub timestamp: u64,
    pub is_alarm: bool,
}

#[derive(sqlx::FromRow, Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Count {
    pub count: i64,
}

impl fmt::Display for ModelRequest {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "request_id: {}, timestamp:{}",
            self.request_id, self.timestamp,
        )
    }
}

impl ModelRequest {
    /// Get the current time in seconds, unix epoch style
    pub fn now() -> u64 {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    /// Get the current time in seconds, unix epoch style, but as an i64, for sqlx
    pub fn now_i64() -> i64 {
        i64::try_from(Self::now()).unwrap_or_default()
    }

    const fn count_query<'a>(push_request: &PushRequest) -> &'a str {
        match push_request {
            PushRequest::Alarm(_) => {
                "SELECT COUNT(*) AS count FROM request WHERE is_alarm = TRUE AND timestamp BETWEEN $1 AND $2 ORDER BY timestamp"
            }
            PushRequest::TestRequest => {
                "SELECT COUNT(*) AS count FROM request WHERE is_alarm = FALSE AND timestamp BETWEEN $1 AND $2 ORDER BY timestamp"
            }
        }
    }

    /// Is the given PushRequest Alarm - used in ModelRequest query
    const fn is_alarm(push_request: &PushRequest) -> bool {
        match push_request {
            PushRequest::Alarm(_) => true,
            PushRequest::TestRequest => false,
        }
    }

    /// Count the number of request made in the past hour, based on type of request
    pub async fn count_past_hour(
        db: &SqlitePool,
        push_request: &PushRequest,
    ) -> Result<i64, AppError> {
        let one_hour = 1
            .hour()
            .round(SpanRound::new().largest(Unit::Second))
            .map_or(0, |i| i.get_seconds());
        let result = sqlx::query_as::<_, Count>(Self::count_query(push_request))
            .bind(Self::now_i64() - one_hour)
            .bind(Self::now_i64())
            .fetch_one(db)
            .await?;
        Ok(result.count)
    }

    // insert a new request with timestamp
    pub async fn insert(db: &SqlitePool, push_request: &PushRequest) -> Result<Self, AppError> {
        let sql = "INSERT INTO request(timestamp, is_alarm) VALUES ($1, $2) RETURNING request_id, is_alarm, timestamp";
        let query = sqlx::query_as::<_, Self>(sql)
            .bind(Self::now_i64())
            .bind(Self::is_alarm(push_request))
            .fetch_one(db)
            .await?;
        Ok(query)
    }

    #[cfg(test)]
    pub async fn test_get_all(db: &SqlitePool) -> Result<Vec<Self>, AppError> {
        let sql = "SELECT * FROM request";
        let result = sqlx::query_as::<_, Self>(sql).fetch_all(db).await?;
        Ok(result)
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used)]
mod tests {
    use crate::tests::{test_cleanup, test_setup};

    use super::*;

    #[tokio::test]
    async fn model_request_add_ok() {
        let (_app_envs, sqlite, uuid) = test_setup().await;

        let now = ModelRequest::now();
        let result = ModelRequest::insert(&sqlite, &PushRequest::Alarm(0)).await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.request_id, 1);
        assert_eq!(result.timestamp, now);
        assert!(result.is_alarm);

        let result = ModelRequest::insert(&sqlite, &PushRequest::TestRequest).await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.request_id, 2);
        assert!(!result.is_alarm);
        assert_eq!(result.timestamp, now);
        test_cleanup(uuid, Some(sqlite)).await;
    }

    #[tokio::test]
    async fn model_request_offset() {
        let (_app_envs, sqlite, uuid) = test_setup().await;

        let now = ModelRequest::now();
        let result = ModelRequest::insert(&sqlite, &PushRequest::Alarm(0)).await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.request_id, 1);
        assert!(result.is_alarm);
        assert_eq!(result.timestamp, now);
        test_cleanup(uuid, Some(sqlite)).await;
    }

    #[tokio::test]
    async fn model_request_get_all_ok() {
        let (_app_envs, sqlite, uuid) = test_setup().await;
        let now = ModelRequest::now();
        for i in 0..4 {
            let sql = "INSERT INTO request(timestamp, is_alarm) VALUES ($1, true)";
            sqlx::query(sql)
                .bind(i64::try_from(now + i).unwrap())
                .execute(&sqlite)
                .await
                .unwrap();
        }

        let result = ModelRequest::test_get_all(&sqlite).await;

        assert!(result.is_ok());
        let result = result.unwrap();

        assert_eq!(result.len(), 4);
        assert_eq!(result[0].timestamp, now);
        assert_eq!(result[0].request_id, 1);

        assert_eq!(result[1].timestamp, now + 1);
        assert_eq!(result[1].request_id, 2);

        assert_eq!(result[2].timestamp, now + 2);
        assert_eq!(result[2].request_id, 3);

        assert_eq!(result[3].timestamp, now + 3);
        assert_eq!(result[3].request_id, 4);

        test_cleanup(uuid, Some(sqlite)).await;
    }

    #[tokio::test]
    // Four requests inserted, two over an hour ago
    async fn model_request_get_last_hour_alarm() {
        let (_app_envs, sqlite, uuid) = test_setup().await;

        for i in 1..=4 {
            let sql = "INSERT INTO request(timestamp, is_alarm) VALUES ($1, true)";
            let timestamp = ModelRequest::now_i64() - (60 * (i * 25));

            sqlx::query(sql)
                .bind(timestamp)
                .execute(&sqlite)
                .await
                .unwrap();
            let sql = "INSERT INTO request(timestamp, is_alarm) VALUES ($1, false)";

            sqlx::query(sql)
                .bind(timestamp)
                .execute(&sqlite)
                .await
                .unwrap();
        }

        let result = ModelRequest::count_past_hour(&sqlite, &PushRequest::Alarm(0)).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 2);
        let result = ModelRequest::count_past_hour(&sqlite, &PushRequest::TestRequest).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 2);

        test_cleanup(uuid, Some(sqlite)).await;
    }
}

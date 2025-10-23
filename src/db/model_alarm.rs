use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::fmt;

use crate::{app_error::AppError, ws_messages::HourMinuteMsg};

#[derive(sqlx::FromRow, Debug, Clone, Serialize, Deserialize)]
pub struct ModelAlarm {
    #[serde(skip_serializing)]
    pub alarm_id: i64,
    pub hour: i8,
    pub minute: i8,
    pub message: Option<String>,
}

impl fmt::Display for ModelAlarm {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "alarm_id: {}, hour:{}, minute:{}, message: {}",
            self.alarm_id,
            self.hour,
            self.minute,
            self.message.as_ref().unwrap_or(&String::new())
        )
    }
}

impl ModelAlarm {
    pub async fn get(sqlite: &SqlitePool) -> Result<Option<Self>, AppError> {
        let sql = "SELECT
    alarm_id, hour, minute,
    CASE
        WHEN message = '' THEN NULL
        ELSE message
    END AS message
FROM
    alarm";
        Ok(sqlx::query_as::<_, Self>(sql)
            .fetch_optional(sqlite)
            .await?)
    }

    pub async fn add(sqlite: &SqlitePool, data: HourMinuteMsg) -> Result<(), AppError> {
        let sql = "INSERT INTO alarm(hour, minute, message) VALUES ($1, $2, $3) RETURNING alarm_id, hour, minute, message";
        sqlx::query_as::<_, Self>(sql)
            .bind(data.hour)
            .bind(data.minute)
            .bind(data.message)
            .fetch_one(sqlite)
            .await?;
        Ok(())
    }

    pub async fn update(sqlite: &SqlitePool, data: HourMinuteMsg) -> Result<(), AppError> {
        let sql = "UPDATE alarm SET hour = $1, minute = $2, message = $3 RETURNING alarm_id, hour, minute, message;";
        sqlx::query_as::<_, Self>(sql)
            .bind(data.hour)
            .bind(data.minute)
            .bind(data.message)
            .fetch_one(sqlite)
            .await?;
        Ok(())
    }

    pub async fn delete(sqlite: &SqlitePool) -> Result<(), AppError> {
        let sql = "DELETE FROM alarm";
        sqlx::query(sql).execute(sqlite).await?;
        Ok(())
    }
}

// ModelAlarm tests
//
/// cargo watch -q -c -w src/ -x 'test model_alarm -- --test-threads=1 --nocapture'
#[cfg(test)]
#[expect(clippy::unwrap_used)]
mod tests {
    use crate::tests::{test_cleanup, test_setup};

    use super::*;

    #[tokio::test]
    async fn model_alarm_add_ok_msg_none() {
        let (_, sqlite, uuid) = test_setup().await;
        let data = HourMinuteMsg::from((10, 10, None));
        let result = ModelAlarm::add(&sqlite, data).await;
        assert!(result.is_ok());
        let result = ModelAlarm::get(&sqlite).await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.alarm_id, 1);
        assert_eq!(result.hour, 10);
        assert_eq!(result.minute, 10);

        assert!(result.message.is_none());
        test_cleanup(uuid, Some(sqlite)).await;
    }

    #[tokio::test]
    async fn model_alarm_add_ok_msg_empty_none() {
        let (_, sqlite, uuid) = test_setup().await;
        let data = HourMinuteMsg::from((10, 10, Some(String::new())));
        let result = ModelAlarm::add(&sqlite, data).await;
        assert!(result.is_ok());
        let result = ModelAlarm::get(&sqlite).await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.alarm_id, 1);
        assert_eq!(result.hour, 10);
        assert_eq!(result.minute, 10);

        assert!(result.message.is_none());
        test_cleanup(uuid, Some(sqlite)).await;
    }

    #[tokio::test]
    async fn model_alarm_add_ok_msg_some() {
        let (_, sqlite, uuid) = test_setup().await;
        let data = HourMinuteMsg::from((10, 10, Some("test".to_owned())));

        let result = ModelAlarm::add(&sqlite, data).await;
        assert!(result.is_ok());
        let result = ModelAlarm::get(&sqlite).await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.alarm_id, 1);
        assert_eq!(result.hour, 10);
        assert_eq!(result.minute, 10);
        assert_eq!(result.message.unwrap(), "test");
        test_cleanup(uuid, Some(sqlite)).await;
    }

    #[tokio::test]
    async fn model_alarm_second_add_err() {
        let (_, sqlite, uuid) = test_setup().await;
        let data = HourMinuteMsg::from((10, 10, None));

        let result = ModelAlarm::add(&sqlite, data).await;
        assert!(result.is_ok());

        let data = HourMinuteMsg::from((10, 10, None));
        let result = ModelAlarm::add(&sqlite, data).await;
        assert!(result.is_err());

        assert_eq!(
            result.unwrap_err().to_string(),
            "Internal Database Error: error returned from database: (code: 1811) only one alarm allowed"
        );

        test_cleanup(uuid, Some(sqlite)).await;
    }

    #[tokio::test]
    async fn model_alarm_update_ok() {
        let (_, sqlite, uuid) = test_setup().await;
        let data = HourMinuteMsg::from((10, 10, None));

        let result = ModelAlarm::add(&sqlite, data).await;
        assert!(result.is_ok());

        assert!(result.is_ok());

        let result = ModelAlarm::get(&sqlite).await;
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.is_some());
        let result = result.unwrap();

        assert_eq!(result.alarm_id, 1);
        assert_eq!(result.hour, 10);
        assert_eq!(result.minute, 10);
        assert!(result.message.is_none());

        let data = HourMinuteMsg::from((11, 11, Some(uuid.to_string())));

        let result = ModelAlarm::update(&sqlite, data).await;
        assert!(result.is_ok());

        let result = ModelAlarm::get(&sqlite).await;
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.alarm_id, 1);
        assert_eq!(result.hour, 11);
        assert_eq!(result.minute, 11);
        assert!(result.message.is_some());
        assert_eq!(result.message.unwrap(), uuid.to_string());

        test_cleanup(uuid, Some(sqlite)).await;
    }

    #[tokio::test]
    async fn model_alarm_add_err_invalid_hour() {
        let (_, sqlite, uuid) = test_setup().await;
        let data = HourMinuteMsg::from((25, 10, None));

        let result = ModelAlarm::add(&sqlite, data).await;

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Internal Database Error: error returned from database: (code: 275) CHECK constraint failed: hour >= 0\n\t\tAND hour <= 23"
        );
        test_cleanup(uuid, Some(sqlite)).await;
    }

    #[tokio::test]
    async fn model_alarm_add_err_invalid_minute() {
        let (_, sqlite, uuid) = test_setup().await;
        let data = HourMinuteMsg::from((10, 60, None));
        let result = ModelAlarm::add(&sqlite, data).await;

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Internal Database Error: error returned from database: (code: 275) CHECK constraint failed: minute >= 0\n\t\tAND minute <= 59"
        );
        test_cleanup(uuid, Some(sqlite)).await;
    }

    #[tokio::test]
    async fn model_alarm_delete_one_ok() {
        let (_, sqlite, uuid) = test_setup().await;
        let data = HourMinuteMsg::from((10, 10, None));
        ModelAlarm::add(&sqlite, data).await.unwrap();

        let result = ModelAlarm::delete(&sqlite).await;
        let alarm = ModelAlarm::get(&sqlite).await.unwrap();

        assert!(result.is_ok());
        assert!(alarm.is_none());
        test_cleanup(uuid, Some(sqlite)).await;
    }
}

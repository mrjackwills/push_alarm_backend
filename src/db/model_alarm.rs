use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::fmt;

use crate::app_error::AppError;

#[derive(sqlx::FromRow, Debug, Clone, Serialize, Deserialize)]
pub struct ModelAlarm {
    #[serde(skip_serializing)]
    pub alarm_id: i64,
    pub hour: u8,
    pub minute: u8,
}

impl fmt::Display for ModelAlarm {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "alarm_id: {}, hour:{}, minute:{}",
            self.alarm_id, self.hour, self.minute
        )
    }
}

impl ModelAlarm {
    pub async fn get(db: &SqlitePool) -> Result<Option<Self>, AppError> {
        let sql = "SELECT * FROM alarm";
        Ok(sqlx::query_as::<_, Self>(sql).fetch_optional(db).await?)
    }

    pub async fn add(db: &SqlitePool, data: (u8, u8)) -> Result<Self, AppError> {
        let sql =
            "INSERT INTO alarm(hour, minute) VALUES ($1, $2) RETURNING alarm_id, hour, minute";
        let query = sqlx::query_as::<_, Self>(sql)
            .bind(data.0)
            .bind(data.1)
            .fetch_one(db)
            .await?;
        Ok(query)
    }

    pub async fn update(db: &SqlitePool, data: (u8, u8)) -> Result<Self, AppError> {
        let sql = "UPDATE alarm SET hour = $1, minute = $2 RETURNING alarm_id, hour, minute;";
        let query = sqlx::query_as::<_, Self>(sql)
            .bind(data.0)
            .bind(data.1)
            .fetch_one(db)
            .await?;
        Ok(query)
    }

    pub async fn delete(db: &SqlitePool) -> Result<(), AppError> {
        let sql = "DELETE FROM alarm";
        sqlx::query(sql).execute(db).await?;
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
    async fn model_alarm_add_ok() {
        let (_, db, uuid) = test_setup().await;
        let data = (10, 10);

        let result = ModelAlarm::add(&db, data).await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.alarm_id, 1);
        assert_eq!(result.hour, 10);
        assert_eq!(result.minute, 10);
        test_cleanup(uuid, Some(db)).await;
    }

    #[tokio::test]
    async fn model_alarm_second_add_err() {
        let (_, db, uuid) = test_setup().await;
        let data = (10, 10);

        let result = ModelAlarm::add(&db, data).await;
        assert!(result.is_ok());

        let result = ModelAlarm::add(&db, data).await;
        assert!(result.is_err());

        assert_eq!(
            result.unwrap_err().to_string(),
            "Internal Database Error: error returned from database: (code: 1811) only one alarm allowed"
        );

        test_cleanup(uuid, Some(db)).await;
    }

    #[tokio::test]
    async fn model_alarm_update_ok() {
        let (_, db, uuid) = test_setup().await;
        let data = (10, 10);

        let result = ModelAlarm::add(&db, data).await;
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.alarm_id, 1);
        assert_eq!(result.hour, 10);
        assert_eq!(result.minute, 10);

        let data: (u8, u8) = (11, 11);

        let result = ModelAlarm::update(&db, data).await;
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.alarm_id, 1);
        assert_eq!(result.hour, 11);
        assert_eq!(result.minute, 11);

        test_cleanup(uuid, Some(db)).await;
    }

    #[tokio::test]
    async fn model_alarm_add_err_invalid_hour() {
        let (_, db, uuid) = test_setup().await;
        let data = (25, 10);

        let result = ModelAlarm::add(&db, data).await;

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Internal Database Error: error returned from database: (code: 275) CHECK constraint failed: hour >= 0\n\t\tAND hour <= 23"
        );
        test_cleanup(uuid, Some(db)).await;
    }

    #[tokio::test]
    async fn model_alarm_add_err_invalid_minute() {
        let (_, db, uuid) = test_setup().await;
        let data = (10, 60);

        let result = ModelAlarm::add(&db, data).await;

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Internal Database Error: error returned from database: (code: 275) CHECK constraint failed: minute >= 0\n\t\tAND minute <= 59"
        );
        test_cleanup(uuid, Some(db)).await;
    }

    #[tokio::test]
    async fn model_alarm_delete_one_ok() {
        let (_, db, uuid) = test_setup().await;
        let data = (10, 10);
        ModelAlarm::add(&db, data).await.unwrap();

        let result = ModelAlarm::delete(&db).await;
        let alarm = ModelAlarm::get(&db).await.unwrap();

        assert!(result.is_ok());
        assert!(alarm.is_none());
        test_cleanup(uuid, Some(db)).await;
    }
}

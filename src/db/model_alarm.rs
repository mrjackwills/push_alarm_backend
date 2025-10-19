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
    pub async fn get(db: &SqlitePool) -> Result<Option<Self>, AppError> {
        let sql = "SELECT * FROM alarm";
        Ok(sqlx::query_as::<_, Self>(sql).fetch_optional(db).await?)
    }

    pub async fn add(db: &SqlitePool, data: HourMinuteMsg) -> Result<Self, AppError> {
        let sql = "INSERT INTO alarm(hour, minute, message) VALUES ($1, $2, $3) RETURNING alarm_id, hour, minute, message";
        let query = sqlx::query_as::<_, Self>(sql)
            .bind(data.hour)
            .bind(data.minute)
            .bind(data.message)
            .fetch_one(db)
            .await?;
        Ok(query)
    }

    pub async fn update(db: &SqlitePool, data: HourMinuteMsg) -> Result<Self, AppError> {
        let sql = "UPDATE alarm SET hour = $1, minute = $2, message = $3 RETURNING alarm_id, hour, minute, message;";
        let query = sqlx::query_as::<_, Self>(sql)
            .bind(data.hour)
            .bind(data.minute)
            .bind(data.message)
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
    use crate::{
        S,
        tests::{test_cleanup, test_setup},
    };

    use super::*;

    #[tokio::test]
    async fn model_alarm_add_ok_msg_none() {
        let (_, db, uuid) = test_setup().await;
        let data = HourMinuteMsg::from((10, 10, None));

        let result = ModelAlarm::add(&db, data).await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.alarm_id, 1);
        assert_eq!(result.hour, 10);
        assert_eq!(result.minute, 10);
        assert_eq!(result.message, None);
        test_cleanup(uuid, Some(db)).await;
    }

    #[tokio::test]
    async fn model_alarm_add_ok_msg_some() {
        let (_, db, uuid) = test_setup().await;
        let data = HourMinuteMsg::from((10, 10, Some("test".to_owned())));

        let result = ModelAlarm::add(&db, data).await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.alarm_id, 1);
        assert_eq!(result.hour, 10);
        assert_eq!(result.minute, 10);
        assert_eq!(result.message.unwrap(), "test");
        test_cleanup(uuid, Some(db)).await;
    }

    #[tokio::test]
    async fn model_alarm_second_add_err() {
        let (_, db, uuid) = test_setup().await;
        let data = HourMinuteMsg::from((10, 10, None));

        let result = ModelAlarm::add(&db, data).await;
        assert!(result.is_ok());

        let data = HourMinuteMsg::from((10, 10, None));
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
        let data = HourMinuteMsg::from((10, 10, None));

        let result = ModelAlarm::add(&db, data).await;
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.alarm_id, 1);
        assert_eq!(result.hour, 10);
        assert_eq!(result.minute, 10);

        let data = HourMinuteMsg::from((11, 11, Some(S!("test"))));

        let result = ModelAlarm::update(&db, data).await;
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.alarm_id, 1);
        assert_eq!(result.hour, 11);
        assert_eq!(result.minute, 11);
        assert_eq!(result.message.unwrap(), "test");

        test_cleanup(uuid, Some(db)).await;
    }

    #[tokio::test]
    async fn model_alarm_add_err_invalid_hour() {
        let (_, db, uuid) = test_setup().await;
        let data = HourMinuteMsg::from((25, 10, None));

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
        let data = HourMinuteMsg::from((10, 60, None));
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
        let data = HourMinuteMsg::from((10, 10, None));
        ModelAlarm::add(&db, data).await.unwrap();

        let result = ModelAlarm::delete(&db).await;
        let alarm = ModelAlarm::get(&db).await.unwrap();

        assert!(result.is_ok());
        assert!(alarm.is_none());
        test_cleanup(uuid, Some(db)).await;
    }
}

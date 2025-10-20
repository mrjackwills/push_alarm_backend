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
    pub message: String,
}

impl fmt::Display for ModelAlarm {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "alarm_id: {}, hour:{}, minute:{}, message: {}",
            self.alarm_id, self.hour, self.minute, self.message
        )
    }
}

impl ModelAlarm {
    pub async fn get(db: &SqlitePool) -> Result<Option<Self>, AppError> {
        let sql = "SELECT
    alarm_id, hour, minute,
    CASE
        WHEN message IS NULL OR message = '' THEN (SELECT stratergy FROM oblique_strategies ORDER BY RANDOM() LIMIT 1)
        ELSE message
    END AS message
FROM
    alarm";
        // fix this so that if message is none, return a random unqiue stratergy
        Ok(sqlx::query_as::<_, Self>(sql).fetch_optional(db).await?)
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
    use std::collections::HashSet;

    use crate::{
        db::ModelObliqueStratergy,
        tests::{test_cleanup, test_setup},
    };

    use super::*;

    fn get_all_strats() -> HashSet<String> {
        include_str!("../db/oblique.txt")
            .lines()
            .map(std::borrow::ToOwned::to_owned)
            .collect::<HashSet<_>>()
    }

    #[tokio::test]
    async fn model_alarm_add_ok_msg_none() {
        let (_, sqlite, uuid) = test_setup().await;
        let data = HourMinuteMsg::from((10, 10, None));
        ModelObliqueStratergy::seed_stratergies(&sqlite)
            .await
            .unwrap();

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

        let all_strats = get_all_strats();
        assert!(all_strats.contains(&result.message));
        test_cleanup(uuid, Some(sqlite)).await;
    }

    #[tokio::test]
    async fn model_alarm_add_ok_msg_some() {
        let (_, sqlite, uuid) = test_setup().await;
        let data = HourMinuteMsg::from((10, 10, Some("test".to_owned())));
        ModelObliqueStratergy::seed_stratergies(&sqlite)
            .await
            .unwrap();

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
        assert_eq!(result.message, "test");
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
        ModelObliqueStratergy::seed_stratergies(&sqlite)
            .await
            .unwrap();
        let all_strats = get_all_strats();

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
        assert!(all_strats.contains(&result.message));

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
        assert_eq!(result.message, uuid.to_string());

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

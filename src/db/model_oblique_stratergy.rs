use serde::Deserialize;
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::app_error::AppError;

#[derive(sqlx::FromRow, Debug, Clone, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ModelObliqueStratergy {
    pub stratergy: String,
}

impl ModelObliqueStratergy {
    async fn insert(
        transaction: &mut Transaction<'_, Sqlite>,
        stratergy: &str,
    ) -> Result<(), AppError> {
        let sql = "INSERT INTO oblique_strategies (stratergy) VALUES($1) ON CONFLICT(stratergy) DO NOTHING";
        sqlx::query(sql)
            .bind(stratergy)
            .execute(&mut **transaction)
            .await?;
        Ok(())
    }

    pub async fn seed_stratergies(sqlite: &SqlitePool) -> Result<(), AppError> {
        let all_stratergies = include_str!("./oblique.txt");
        let mut transaction = sqlite.begin().await?;
        for stratergy in all_stratergies.lines() {
            Self::insert(&mut transaction, stratergy).await?;
        }
        transaction.commit().await?;

        Ok(())
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used)]
mod tests {

    use crate::{
        db::{create_tables, file_exists, get_db},
        tests::{gen_app_envs, test_cleanup},
    };
    use uuid::Uuid;

    use super::*;

    #[tokio::test]
    async fn model_oblique_stratergy_seed() {
        let uuid = Uuid::new_v4();

        let app_envs = gen_app_envs(uuid);

        file_exists(&app_envs.location_sqlite);
        let db = get_db(&app_envs).await.unwrap();
        create_tables(&db).await;

        let result = ModelObliqueStratergy::seed_stratergies(&db).await;

        assert!(result.is_ok());

        // Make sure conflicts are ignored, in that they don't cause errors
        let result = ModelObliqueStratergy::seed_stratergies(&db).await;

        assert!(result.is_ok());
        test_cleanup(uuid, Some(db)).await;
    }
}

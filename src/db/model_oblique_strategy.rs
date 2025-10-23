use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::app_error::AppError;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, sqlx::FromRow)]
pub struct ModelObliqueStrategy {
    pub strategy: String,
}

impl ModelObliqueStrategy {
    async fn insert(
        transaction: &mut Transaction<'_, Sqlite>,
        trategy: &str,
    ) -> Result<(), AppError> {
        let sql =
            "INSERT INTO oblique_strategies (strategy) VALUES($1) ON CONFLICT(strategy) DO NOTHING";
        sqlx::query(sql)
            .bind(trategy)
            .execute(&mut **transaction)
            .await?;
        Ok(())
    }

    pub async fn get_random(sqlite: &SqlitePool) -> Result<String, AppError> {
        Ok(
            sqlx::query_as::<_, Self>("SELECT * FROM oblique_strategies ORDER BY RANDOM() LIMIT 1")
                .fetch_one(sqlite)
                .await?
                .strategy,
        )
    }

    pub async fn seed_stratergies(sqlite: &SqlitePool) -> Result<(), AppError> {
        let all_stratergies = include_str!("./oblique.txt");
        let mut transaction = sqlite.begin().await?;
        for trategy in all_stratergies.lines() {
            Self::insert(&mut transaction, trategy).await?;
        }
        transaction.commit().await?;

        Ok(())
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used)]
mod tests {

    use std::collections::HashSet;

    use crate::{
        db::{create_tables, file_exists, get_db},
        tests::{gen_app_envs, test_cleanup},
    };
    use uuid::Uuid;

    use super::*;

    #[tokio::test]
    async fn model_oblique_trategy_seed() {
        let uuid = Uuid::new_v4();

        let app_envs = gen_app_envs(uuid);

        file_exists(&app_envs.location_sqlite);
        let sqlite = get_db(&app_envs).await.unwrap();
        create_tables(&sqlite).await;

        let result = ModelObliqueStrategy::seed_stratergies(&sqlite).await;

        assert!(result.is_ok());

        // Make sure conflicts are ignored, in that they don't cause errors
        let result = ModelObliqueStrategy::seed_stratergies(&sqlite).await;

        assert!(result.is_ok());
        test_cleanup(uuid, Some(sqlite)).await;
    }

    #[tokio::test]
    async fn model_oblique_trategy_get() {
        let uuid = Uuid::new_v4();

        let app_envs = gen_app_envs(uuid);

        file_exists(&app_envs.location_sqlite);
        let sqlite = get_db(&app_envs).await.unwrap();
        create_tables(&sqlite).await;

        ModelObliqueStrategy::seed_stratergies(&sqlite)
            .await
            .unwrap();

        let mut set = HashSet::new();

        for _ in 0..=40 {
            let result = ModelObliqueStrategy::get_random(&sqlite).await;
            assert!(result.is_ok());
            let result = result.unwrap();
            assert!(!result.is_empty());
            set.insert(result);
        }

        // This could fail
        assert!(set.len() > 15);

        test_cleanup(uuid, Some(sqlite)).await;
    }
}

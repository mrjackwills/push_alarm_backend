use reqwest::Client;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::net::IpAddr;
use url::Url;

use crate::{app_env::AppEnv, app_error::AppError, db::ModelRequest};

/// Pushover api url
const URL: &str = "https://api.pushover.net/1/messages.json";

type Params<'a> = [(&'a str, String); 4];

#[derive(Debug, Serialize, Deserialize)]
/// Response from pushover api, currently not actually doing anything with it
struct PostRequest {
    status: usize,
    request: String,
}

/// Response from the what's my ip api
#[derive(Debug, Serialize, Deserialize)]
struct IpResponse {
    ip: IpAddr,
}

pub enum PushRequest {
    Alarm(u8),
    Test(String),
}

impl PushRequest {
    /// How many requests can be made in the previous hour
    const fn hour_limit(&self) -> i64 {
        match self {
            Self::Alarm(_) => 60,
            Self::Test(_) => 10,
        }
    }

    /// Get the reqwest client, in reality should never actually fail
    fn get_client() -> Result<Client, AppError> {
        Ok(reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_millis(5000))
            .gzip(true)
            .brotli(true)
            .user_agent(format!(
                "{}/{}",
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION")
            ))
            .build()?)
    }

    #[cfg(not(test))]
    /// The actual request via PushOver api
    async fn send_request(url: Url) -> Result<PostRequest, AppError> {
        let client = Self::get_client()?;
        Ok(client.post(url).send().await?.json::<PostRequest>().await?)
    }

    #[cfg(test)]
    #[allow(clippy::unused_async)]
    async fn send_request(_: Url) -> Result<PostRequest, AppError> {
        let _client = Self::get_client()?;
        Ok(PostRequest {
            status: 1,
            request: "request".to_owned(),
        })
    }

    const fn get_priority<'a>(&self) -> &'a str {
        match self {
            Self::Alarm(_) => "1",
            Self::Test(_) => "0",
        }
    }

    /// Generate the params, aka the message
    fn gen_params<'a>(&self, app_envs: &AppEnv) -> Params<'a> {
        let mut params = [
            ("token", app_envs.token_app.clone()),
            ("user", app_envs.token_user.clone()),
            ("message", String::new()),
            ("priority", self.get_priority().to_owned()),
        ];

        match self {
            Self::Alarm(index) => {
                params[2].1 = format!("Wake up, loop {index}");
            }
            Self::Test(message) => {
                params[2].1 = message.to_owned();
            }
        }
        params
    }

    /// Insert a new request into the database
    async fn insert_request(&self, db: &SqlitePool) -> Result<(), AppError> {
        ModelRequest::insert(db, self).await?;
        Ok(())
    }

    /// Make the request, will check to make sure that haven't made too many request in previous hour
    /// get_ip functions are recursive, to deal with no network at first boot
    pub async fn make_request(&self, app_envs: &AppEnv, db: &SqlitePool) -> Result<(), AppError> {
        let requests_made = ModelRequest::count_past_hour(db, self).await?;

        if requests_made >= self.hour_limit() {
            Err(AppError::TooManyRequests(requests_made))
        } else {
            tracing::debug!("Sending request");
            let params = self.gen_params(app_envs);
            let url = reqwest::Url::parse_with_params(URL, &params)?;
            self.insert_request(db).await?;

            Self::send_request(url).await?;
            // do something with the response here?
            tracing::debug!("Request sent");
            Ok(())
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {

    use super::*;
    use crate::tests::{test_cleanup, test_setup};

    #[tokio::test]
    async fn test_request_generate_params() {
        let (app_envs, db, uuid) = test_setup().await;

        let push_request = PushRequest::Alarm(0);
        let result = push_request.gen_params(&app_envs);

        assert_eq!(result[0], ("token", "test_token_app".to_owned()));
        assert_eq!(result[2], ("message", "Wake up, loop 0".to_owned()));
        assert_eq!(result[1], ("user", "test_token_user".to_owned()));
        assert_eq!(result[3], ("priority", "1".to_owned()));

        let push_request = PushRequest::Alarm(8);
        let result = push_request.gen_params(&app_envs);

        assert_eq!(result[0], ("token", "test_token_app".to_owned()));
        assert_eq!(result[2], ("message", "Wake up, loop 8".to_owned()));
        assert_eq!(result[1], ("user", "test_token_user".to_owned()));
        assert_eq!(result[3], ("priority", "1".to_owned()));

        let push_request = PushRequest::Test("test message".to_owned());
        let result = push_request.gen_params(&app_envs);

        assert_eq!(result[0], ("token", "test_token_app".to_owned()));
        assert_eq!(result[2], ("message", "test message".to_owned()));
        assert_eq!(result[1], ("user", "test_token_user".to_owned()));
        assert_eq!(result[3], ("priority", "0".to_owned()));

        test_cleanup(uuid, Some(db)).await;
    }

    #[tokio::test]
    // Alarm request not made if 60+ requests been made in previous 60 minutes
    async fn test_request_make_request_not_made_alarm() {
        let (app_envs, db, uuid) = test_setup().await;

        for _ in 1..=60 {
            let sql = "INSERT INTO request(timestamp, is_alarm) VALUES ($1, true)";
            sqlx::query(sql)
                .bind(ModelRequest::now_i64())
                .execute(&db)
                .await
                .unwrap();
        }

        let request_len = ModelRequest::test_get_all(&db).await;
        assert!(request_len.is_ok());
        assert_eq!(request_len.unwrap().len(), 60);

        let result = PushRequest::Alarm(0).make_request(&app_envs, &db).await;

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Too many requests made in the past hour: 60"
        );

        let request_len = ModelRequest::test_get_all(&db).await;
        assert!(request_len.is_ok());
        assert_eq!(request_len.unwrap().len(), 60);

        test_cleanup(uuid, Some(db)).await;
    }

    #[tokio::test]
    // Alarm request made as 60+ request were made more than an hour ago
    async fn test_request_make_request_made_alarm() {
        let (app_envs, db, uuid) = test_setup().await;

        for _ in 1..=60 {
            let sql = "INSERT INTO request(timestamp, is_alarm) VALUES ($1, true)";
            sqlx::query(sql).bind(0).execute(&db).await.unwrap();
        }

        let request_len = ModelRequest::test_get_all(&db).await;
        assert!(request_len.is_ok());
        assert_eq!(request_len.unwrap().len(), 60);

        let result = PushRequest::Alarm(0).make_request(&app_envs, &db).await;
        assert!(result.is_ok());

        let request_len = ModelRequest::test_get_all(&db).await;
        assert!(request_len.is_ok());
        assert_eq!(request_len.unwrap().len(), 61);

        test_cleanup(uuid, Some(db)).await;
    }
    #[tokio::test]
    // Test request not made if 10+ requests been made in previous 60 minutes
    async fn test_request_make_request_not_made_test() {
        let (app_envs, db, uuid) = test_setup().await;

        for _ in 1..=10 {
            let sql = "INSERT INTO request(timestamp, is_alarm) VALUES ($1, false)";

            sqlx::query(sql)
                .bind(ModelRequest::now_i64())
                .execute(&db)
                .await
                .unwrap();
        }

        let request_len = ModelRequest::test_get_all(&db).await;
        assert!(request_len.is_ok());
        assert_eq!(request_len.unwrap().len(), 10);

        let result = PushRequest::Test(String::new())
            .make_request(&app_envs, &db)
            .await;

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Too many requests made in the past hour: 10"
        );
        let request_len = ModelRequest::test_get_all(&db).await;
        assert!(request_len.is_ok());
        assert_eq!(request_len.unwrap().len(), 10);

        test_cleanup(uuid, Some(db)).await;
    }

    #[tokio::test]
    // Request made, and inserted into db
    async fn test_request_make_request_count() {
        let (app_envs, db, uuid) = test_setup().await;

        for _ in 1..=10 {
            let sql = "INSERT INTO request(timestamp, is_alarm) VALUES ($1, true)";
            sqlx::query(sql).bind(0).execute(&db).await.unwrap();
        }

        let request_len = ModelRequest::test_get_all(&db).await;
        assert!(request_len.is_ok());
        assert_eq!(request_len.unwrap().len(), 10);

        let result = PushRequest::Test(String::new())
            .make_request(&app_envs, &db)
            .await;
        assert!(result.is_ok());

        let request_len = ModelRequest::test_get_all(&db).await;
        assert!(request_len.is_ok());
        assert_eq!(request_len.unwrap().len(), 11);

        test_cleanup(uuid, Some(db)).await;
    }

    #[tokio::test]
    // Request made, and inserted into db
    async fn test_request_make_request() {
        let (app_envs, db, uuid) = test_setup().await;

        let request_len = ModelRequest::test_get_all(&db).await;
        assert!(request_len.is_ok());
        assert_eq!(request_len.unwrap().len(), 0);

        let result = PushRequest::Alarm(0).make_request(&app_envs, &db).await;

        assert!(result.is_ok());

        let request_len = ModelRequest::test_get_all(&db).await;
        assert!(request_len.is_ok());
        assert_eq!(request_len.unwrap().len(), 1);

        test_cleanup(uuid, Some(db)).await;
    }
}

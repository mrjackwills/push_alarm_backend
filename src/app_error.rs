use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("'{0}' - sql file should end '.db'")]
    DbNameInvalid(String),
    #[error("missing env: '{0}'")]
    MissingEnv(String),
    #[error("Reqwest Error")]
    Reqwest(#[from] reqwest::Error),
    #[error("Internal Database Error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("WS Connect: {0}")]
    TungsteniteConnect(String),
    #[error("Url parsing error: {0}")]
    Url(#[from] url::ParseError),
    #[error("Invalid WS Status Code")]
    WsStatus,
    #[error("Too many requests made in the past hour: {0}")]
    TooManyRequests(i64),
}

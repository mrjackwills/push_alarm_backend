use std::{collections::HashMap, env, fmt, time::SystemTime};
use time_tz::timezones;

use crate::app_error::AppError;

type EnvHashMap = HashMap<String, String>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct EnvTimeZone(String);

impl EnvTimeZone {
    pub fn new(x: impl Into<String>) -> Self {
        let x = x.into();
        if timezones::get_by_name(&x).is_some() {
            Self(x)
        } else {
            Self("Etc/UTC".into())
        }
    }
}

impl fmt::Display for EnvTimeZone {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone)]
pub struct AppEnv {
    pub location_sqlite: String,
    pub log_level: tracing::Level,
    pub start_time: SystemTime,
    pub token_app: String,
    pub token_user: String,
    pub timezone: EnvTimeZone,
    pub ws_address: String,
    pub ws_apikey: String,
    pub ws_password: String,
    pub ws_token_address: String,
}

impl AppEnv {
    /// Parse "true" or "false" to bool, else false
    fn parse_boolean(key: &str, map: &EnvHashMap) -> bool {
        map.get(key).is_some_and(|value| value == "true")
    }

    /// Make sure database file ends .db
    fn parse_db_name(key: &str, map: &EnvHashMap) -> Result<String, AppError> {
        match map.get(key) {
            None => Err(AppError::MissingEnv(key.into())),
            Some(value) => {
                if std::path::Path::new(value)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("db"))
                {
                    return Ok(value.into());
                }
                Err(AppError::DbNameInvalid(key.into()))
            }
        }
    }

    fn parse_string(key: &str, map: &EnvHashMap) -> Result<String, AppError> {
        map.get(key)
            .map_or(Err(AppError::MissingEnv(key.into())), |value| {
                Ok(value.into())
            })
    }

    /// Check that a given timezone is valid, else return UTC
    fn parse_timezone(map: &EnvHashMap) -> EnvTimeZone {
        EnvTimeZone::new(
            map.get("TZ")
                .map_or_else(String::new, std::borrow::ToOwned::to_owned),
        )
    }

    /// Parse debug and/or trace into tracing level
    fn parse_log(map: &EnvHashMap) -> tracing::Level {
        if Self::parse_boolean("LOG_TRACE", map) {
            tracing::Level::TRACE
        } else if Self::parse_boolean("LOG_DEBUG", map) {
            tracing::Level::DEBUG
        } else {
            tracing::Level::INFO
        }
    }

    /// Load, and parse .env file, return `AppEnv`
    fn generate() -> Result<Self, AppError> {
        let env_map = env::vars()
            .map(|i| (i.0, i.1))
            .collect::<HashMap<String, String>>();

        Ok(Self {
            location_sqlite: Self::parse_db_name("LOCATION_SQLITE", &env_map)?,
            log_level: Self::parse_log(&env_map),
            start_time: SystemTime::now(),
            timezone: Self::parse_timezone(&env_map),
            token_app: Self::parse_string("TOKEN_APP", &env_map)?,
            token_user: Self::parse_string("TOKEN_USER", &env_map)?,
            ws_address: Self::parse_string("WS_ADDRESS", &env_map)?,
            ws_apikey: Self::parse_string("WS_APIKEY", &env_map)?,
            ws_password: Self::parse_string("WS_PASSWORD", &env_map)?,
            ws_token_address: Self::parse_string("WS_TOKEN_ADDRESS", &env_map)?,
        })
    }

    pub fn get() -> Self {
        let local_env = ".env";
        let app_env = "/app_env/.env";

        let env_path = if std::fs::exists(app_env).unwrap_or_default() {
            app_env
        } else if std::fs::exists(local_env).unwrap_or_default() {
            local_env
        } else {
            println!("\n\x1b[31munable to load env file\x1b[0m\n");
            std::process::exit(1);
        };

        dotenvy::from_path(env_path).ok();
        match Self::generate() {
            Ok(s) => s,
            Err(e) => {
                println!("\n\x1b[31m{e}\x1b[0m\n");
                std::process::exit(1);
            }
        }
    }
}

/// Run tests with
///
/// cargo watch -q -c -w src/ -x 'test env_ -- --nocapture'
#[cfg(test)]
#[expect(clippy::unwrap_used)]
mod tests {
    use crate::S;

    use super::*;

    #[tokio::test]
    async fn env_missing_env() {
        let mut map = HashMap::new();
        map.insert(S!("not_fish"), S!("not_fish"));

        let result = AppEnv::parse_string("fish", &map);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "missing env: 'fish'");
    }

    #[tokio::test]
    async fn env_parse_string_valid() {
        let mut map = HashMap::new();
        map.insert(S!("LOCATION_SQLITE"), S!("/alarms.db"));

        let result = AppEnv::parse_string("LOCATION_SQLITE", &map).unwrap();

        assert_eq!(result, "/alarms.db");
    }

    #[tokio::test]
    async fn env_parse_boolean_ok() {
        let mut map = HashMap::new();
        map.insert(S!("valid_true"), S!("true"));
        map.insert(S!("valid_false"), S!("false"));
        map.insert(S!("invalid_but_false"), S!("as"));

        let result01 = AppEnv::parse_boolean("valid_true", &map);
        let result02 = AppEnv::parse_boolean("valid_false", &map);
        let result03 = AppEnv::parse_boolean("invalid_but_false", &map);
        let result04 = AppEnv::parse_boolean("missing", &map);

        assert!(result01);
        assert!(!result02);
        assert!(!result03);
        assert!(!result04);
    }

    #[tokio::test]
    async fn env_parse_db_location_ok() {
        let mut map = HashMap::new();
        map.insert(S!("LOCATION_SQLITE"), S!("file.db"));

        let result = AppEnv::parse_db_name("LOCATION_SQLITE", &map);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "file.db");

        let mut map = HashMap::new();
        map.insert(S!("LOCATION_SQLITE"), S!("some/nested/location/file.db"));

        let result = AppEnv::parse_db_name("LOCATION_SQLITE", &map);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "some/nested/location/file.db");
    }

    #[tokio::test]
    async fn env_parse_db_location_format_err() {
        let mut map = HashMap::new();
        map.insert(S!("LOCATION_SQLITE"), S!("file.sql"));

        let result = AppEnv::parse_db_name("LOCATION_SQLITE", &map);

        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::DbNameInvalid(value) => assert_eq!(value, "LOCATION_SQLITE"),
            _ => unreachable!(),
        }
    }

    #[tokio::test]
    async fn env_parse_db_location_missing_err() {
        let map = HashMap::new();

        let result = AppEnv::parse_db_name("LOCATION_SQLITE", &map);

        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::MissingEnv(value) => assert_eq!(value, "LOCATION_SQLITE"),
            _ => unreachable!(),
        }
    }

    #[test]
    fn env_parse_log_valid() {
        let map = HashMap::from([(S!("RANDOM_STRING"), S!("123"))]);

        let result = AppEnv::parse_log(&map);

        assert_eq!(result, tracing::Level::INFO);

        let map = HashMap::from([(S!("LOG_DEBUG"), S!("false"))]);

        let result = AppEnv::parse_log(&map);

        assert_eq!(result, tracing::Level::INFO);

        let map = HashMap::from([(S!("LOG_TRACE"), S!("false"))]);

        let result = AppEnv::parse_log(&map);

        assert_eq!(result, tracing::Level::INFO);

        let map = HashMap::from([
            (S!("LOG_DEBUG"), S!("false")),
            (S!("LOG_TRACE"), S!("false")),
        ]);

        let result = AppEnv::parse_log(&map);

        assert_eq!(result, tracing::Level::INFO);

        let map = HashMap::from([
            (S!("LOG_DEBUG"), S!("true")),
            (S!("LOG_TRACE"), S!("false")),
        ]);

        let result = AppEnv::parse_log(&map);

        assert_eq!(result, tracing::Level::DEBUG);

        let map = HashMap::from([(S!("LOG_DEBUG"), S!("true")), (S!("LOG_TRACE"), S!("true"))]);

        let result = AppEnv::parse_log(&map);

        assert_eq!(result, tracing::Level::TRACE);

        let map = HashMap::from([
            (S!("LOG_DEBUG"), S!("false")),
            (S!("LOG_TRACE"), S!("true")),
        ]);

        let result = AppEnv::parse_log(&map);

        assert_eq!(result, tracing::Level::TRACE);
    }

    #[tokio::test]
    async fn env_parse_timezone_ok() {
        let mut map = HashMap::new();
        map.insert(S!("TZ"), S!("America/New_York"));

        let result = AppEnv::parse_timezone(&map);

        assert_eq!(result.0, "America/New_York");

        let mut map = HashMap::new();
        map.insert(S!("TZ"), S!("Europe/Berlin"));

        let result = AppEnv::parse_timezone(&map);

        assert_eq!(result.0, "Europe/Berlin");

        let map = HashMap::new();

        let result = AppEnv::parse_timezone(&map);

        assert_eq!(result.0, "Etc/UTC");
    }

    #[tokio::test]
    async fn env_parse_timezone_err() {
        let mut map = HashMap::new();
        map.insert(S!("TIMEZONE"), S!("america/New_York"));

        let result = AppEnv::parse_timezone(&map);

        assert_eq!(result.0, "Etc/UTC");

        // No timezone present

        let map = HashMap::new();
        let result = AppEnv::parse_timezone(&map);

        assert_eq!(result.0, "Etc/UTC");
    }
    // WHy?
    #[tokio::test]
    async fn env_panic_appenv() {
        let result = AppEnv::generate();

        assert!(result.is_err());

        dotenvy::dotenv().ok();

        let result = AppEnv::generate();

        assert!(result.is_ok());
    }
}

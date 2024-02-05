use serde::{de, Deserialize, Deserializer};
use std::ops::RangeInclusive;
use time_tz::timezones;
pub struct IncomingSerializer;

impl IncomingSerializer {
    /// Check value is in given range
    fn in_range<'de, D>(deserializer: D, range: RangeInclusive<u8>) -> Result<u8, D::Error>
    where
        D: Deserializer<'de>,
    {
        let parsed = u8::deserialize(deserializer)?;
        if !range.contains(&parsed) {
            return Err(de::Error::custom(format!(
                "{parsed}, not in range {range:?}"
            )));
        }
        Ok(parsed)
    }

    /// Allow only u8s from 0 to 23
    pub fn hour<'de, D>(deserializer: D) -> Result<u8, D::Error>
    where
        D: Deserializer<'de>,
    {
        let range = 0..=23u8;
        Self::in_range(deserializer, range)
    }

    /// Allow only u8s from 0 to 59
    pub fn minute<'de, D>(deserializer: D) -> Result<u8, D::Error>
    where
        D: Deserializer<'de>,
    {
        let range = 0..=59u8;
        Self::in_range(deserializer, range)
    }

    /// Test request message can only be 100 chars max
    pub fn message<'de, D>(deserializer: D) -> Result<String, D::Error>
    where
        D: Deserializer<'de>,
    {
        let parsed = String::deserialize(deserializer)?;
        if parsed.chars().count() <= 100 {
            Ok(parsed)
        } else {
            Err(de::Error::custom("message too long"))
        }
    }

    /// Use timezones crate to make sure is valid timezone
    pub fn timezone<'de, D>(deserializer: D) -> Result<String, D::Error>
    where
        D: Deserializer<'de>,
    {
        let parsed = String::deserialize(deserializer)?;
        match timezones::get_by_name(&parsed) {
            Some(_) => Ok(parsed),
            None => Err(de::Error::custom("unknown timezone")),
        }
    }
}

/// incoming_serializer
///
/// cargo watch -q -c -w src/ -x 'test incoming_serializer -- --test-threads=1 --nocapture'
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use serde::de::value::{Error as ValueError, StringDeserializer, U8Deserializer};
    use serde::de::IntoDeserializer;

    use super::*;

    #[test]
    fn incoming_serializer_minute_err() {
        let deserializer: U8Deserializer<ValueError> = 60u8.into_deserializer();
        let result = IncomingSerializer::minute(deserializer);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "60, not in range 0..=59");
    }

    #[test]
    fn incoming_serializer_minute_ok() {
        let deserializer: U8Deserializer<ValueError> = 30u8.into_deserializer();
        let result = IncomingSerializer::minute(deserializer);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 30u8);
    }

    #[test]
    fn incoming_serializer_hour_err() {
        let deserializer: U8Deserializer<ValueError> = 24u8.into_deserializer();
        let result = IncomingSerializer::hour(deserializer);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "24, not in range 0..=23");
    }

    #[test]
    fn incoming_serializer_hour_ok() {
        let deserializer: U8Deserializer<ValueError> = 23u8.into_deserializer();
        let result = IncomingSerializer::hour(deserializer);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 23u8);
    }

    #[test]
    fn incoming_serializer_timezone_err() {
        let deserializer: StringDeserializer<ValueError> =
            "America/NEwYork".to_owned().into_deserializer();
        let result = IncomingSerializer::timezone(deserializer);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "unknown timezone");

        let deserializer: StringDeserializer<ValueError> =
            "America/New_York".to_lowercase().into_deserializer();
        let result = IncomingSerializer::timezone(deserializer);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "unknown timezone");
    }

    #[test]
    fn incoming_serializer_timezone_ok() {
        let deserializer: StringDeserializer<ValueError> =
            "America/New_York".to_owned().into_deserializer();
        let result = IncomingSerializer::timezone(deserializer);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "America/New_York");
    }

    #[test]
    fn incoming_serializer_message_err() {
        let deserializer: StringDeserializer<ValueError> = "a".repeat(101).into_deserializer();
        let result = IncomingSerializer::message(deserializer);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "message too long");
    }

    #[test]
    fn incoming_serializer_message_ok() {
        let deserializer: StringDeserializer<ValueError> = "A message shorter than 100 chars"
            .to_owned()
            .into_deserializer();
        let result = IncomingSerializer::message(deserializer);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "A message shorter than 100 chars");
    }
}

use super::serializer::IncomingSerializer as is;

use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub enum MessageValues {
    Valid(ParsedMessage, String),
    Invalid(ErrorData),
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case", tag = "name", content = "body")]
pub enum ParsedMessage {
    AlarmAdd(HourMinute),
    AlarmDelete,
    AlarmUpdate(HourMinute),
    Restart,
    Status,
    TestRequest(TestRequest),
    TimeZone(TimeZone),
}

#[derive(Deserialize, Debug, Serialize)]
pub struct TestRequest {
    #[serde(deserialize_with = "is::message")]
    pub message: String,
}

#[derive(Deserialize, Debug, Serialize)]
pub struct HourMinute {
    #[serde(deserialize_with = "is::hour")]
    pub hour: u8,
    #[serde(deserialize_with = "is::minute")]
    pub minute: u8,
}

#[derive(Deserialize, Debug, Serialize)]
pub struct TimeZone {
    #[serde(deserialize_with = "is::timezone")]
    pub zone: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
struct StructuredMessage {
    data: Option<ParsedMessage>,
    error: Option<ErrorData>,
    unique: String,
}

// TODO - this is, at the moment, pointless
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case", tag = "error", content = "message")]
pub enum ErrorData {
    Something(String),
}

// Change this to a Result<MessageValues, AppError>?
pub fn to_struct(input: &str) -> Option<MessageValues> {
    let user_serialized = serde_json::from_str::<StructuredMessage>(input);
    if let Ok(data) = user_serialized {
        if let Some(message) = data.error {
            return Some(MessageValues::Invalid(message));
        }
        if let Some(message) = data.data {
            return Some(MessageValues::Valid(message, data.unique));
        }
        None
    } else {
        let error_serialized = serde_json::from_str::<ErrorData>(input);
        error_serialized.map_or(None, |data| Some(MessageValues::Invalid(data)))
    }
}
// pub fn to_struct(input: &str) -> Option<MessageValues> {
//     let user_serialized = serde_json::from_str::<StructuredMessage>(input);
//     if let Ok(data) = user_serialized {
//         if let Some(data) = data.error {
//             return Some(MessageValues::Invalid(data));
//         }
//         if let Some(data) = data.data {
// 			return Some(MessageValues::Valid((data, data.unique)));
//         }
//         None
//     } else {
//         let error_serialized = serde_json::from_str::<ErrorData>(input);
//         error_serialized.map_or_else(
//             |_| {
//                 debug!("not a known input message");
//                 None
//             },
//             |data| {
//                 debug!("Matched error_serialized data");
//                 Some(MessageValues::Invalid(data))
//             },
//         )
//     }
// }

/// message_incoming
///
/// cargo watch -q -c -w src/ -x 'test message_incoming -- --test-threads=1 --nocapture'
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::too_many_lines)]
mod tests {
    use super::*;

    #[test]
    fn message_incoming_parse_invalid() {
        let data = "";
        let result = to_struct(data);
        assert!(result.is_none());

        let data = "{}";
        let result = to_struct(data);
        assert!(result.is_none());
    }

    #[test]
    fn message_incoming_parse_alarm_add_valid() {
        let data = r#" { "data": { "name": "alarm_add", "body": { "hour": 6, "minute": 15 } }, "unique": "random_string" }"#;

        let result = to_struct(data);

        assert!(result.is_some());
        let result = result.unwrap();
        match result {
            MessageValues::Valid(ParsedMessage::AlarmAdd(data), unique) => {
                assert_eq!(data.hour, 6);
                assert_eq!(data.minute, 15);
                assert_eq!(unique, "random_string");
            }
            _ => unreachable!("Shouldn't have matched this"),
        };
    }

    #[test]
    fn message_incoming_parse_update_alarm_valid() {
        let data = r#" { "data": { "name" :"alarm_update", "body": { "hour": 6, "minute": 15 } }, "unique": "random_string" }"#;
        let result = to_struct(data);

        assert!(result.is_some());
        let result = result.unwrap();
        match result {
            MessageValues::Valid(ParsedMessage::AlarmUpdate(data), _) => {
                assert_eq!(data.hour, 6);
                assert_eq!(data.minute, 15);
            }
            _ => unreachable!("Shouldn't have matched this"),
        };
    }

    fn test_is_none(json: &str) {
        let result = to_struct(json);
        assert!(result.is_none());
    }

    #[test]
    fn message_incoming_parse_alarm_add_invalid() {
        // No body
        test_is_none(r#"{ "data": { "name": "alarm_add" }, "unique":"true"}"#);

        // Empty body
        test_is_none(
            r#"{ "data": { "name": "alarm_add", "body": "" }, "unique": "random_string" }"#,
        );

        // Empty body object
        test_is_none(
            r#"{ "data": { "name": "alarm_add", "body": { } }, "unique": "random_string" }"#,
        );

        // No hours
        test_is_none(
            r#"{ "data": { "name": "alarm_add", "body": { "minute": 6 } }, "unique": "random_string"}"#,
        );

        // invalid hours - number as string
        test_is_none(
            r#" { "data": { "name": "alarm_add", "body": { "hour": "6", "minute": 4 } }, "unique": "random_string" }"#,
        );

        // invalid hours - string
        test_is_none(
            r#" { "data": { "name": "alarm_add", "body": { "hour": "string", "minute": 4 } }, "unique": "random_string" }"#,
        );

        // No minute
        test_is_none(
            r#" { "data": { "name": "alarm_add", "body": { "hour" :6,} }, "unique": "random_string" }"#,
        );

        // invalid minute - number as string
        test_is_none(
            r#" { "data": { "name": "alarm_add", "body": { "hour" :6, "minute": "4" } }, "unique": "random_string" }"#,
        );

        // invalid minute - string
        test_is_none(
            r#" { "data": { "name": "alarm_add", "body": { "hour": 6, "minute": "string" } }, "unique": "random_string" }"#,
        );

        // invalid minute- > 59
        test_is_none(
            r#" { "data": { "name": "alarm_add", "body": { "hour": 6, "minute": 60 } }, "unique": "random_string"}"#,
        );

        // invalid unique
        test_is_none(
            r#" { "data": { "name": "alarm_add", "body": { "hour": 6, "minute": 9 } }, "unique": 1 }"#,
        );
        test_is_none(
            r#" { "data": { "name": "alarm_add", "body": { "hour": 6, "minute": 9 } }, "unique": true }"#,
        );
    }

    #[test]
    fn message_incoming_parse_alarm_update_invalid() {
        // No body
        test_is_none(r#"{ "data": { "name": "alarm_update" }, "unique":"true"}"#);

        // Empty body
        test_is_none(
            r#"{ "data": { "name": "alarm_update", "body": "" }, "unique": "random_string" }"#,
        );

        // Empty body object
        test_is_none(
            r#"{ "data": { "name": "alarm_update", "body": { } }, "unique": "random_string" }"#,
        );

        // No hours
        test_is_none(
            r#"{ "data": { "name": "alarm_update", "body": { "minute": 6 } }, "unique": "random_string"}"#,
        );

        // invalid hours - number as string
        test_is_none(
            r#" { "data": { "name": "alarm_update", "body": { "hour": "6", "minute": 4 } }, "unique": "random_string" }"#,
        );

        // invalid hours - string
        test_is_none(
            r#" { "data": { "name": "alarm_update", "body": { "hour": "string", "minute": 4 } }, "unique": "random_string" }"#,
        );

        // No minute
        test_is_none(
            r#" { "data": { "name": "alarm_update", "body": { "hour" :6,} }, "unique": "random_string" }"#,
        );

        // invalid minute - number as string
        test_is_none(
            r#" { "data": { "name": "alarm_update", "body": { "hour" :6, "minute": "4" } }, "unique": "random_string" }"#,
        );

        // invalid minute - string
        test_is_none(
            r#" { "data": { "name": "alarm_update", "body": { "hour": 6, "minute": "string" } }, "unique": "random_string" }"#,
        );

        // invalid minute- > 59
        test_is_none(
            r#" { "data": { "name": "alarm_update", "body": { "hour": 6, "minute": 60 } }, "unique": "random_string"}"#,
        );

        // invalid unique
        test_is_none(
            r#" { "data": { "name": "alarm_update", "body": { "hour": 6, "minute": 9 } }, "unique": 1 }"#,
        );
        test_is_none(
            r#" { "data": { "name": "alarm_update", "body": { "hour": 6, "minute": 9 } }, "unique": true }"#,
        );
    }
}

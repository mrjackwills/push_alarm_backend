use serde::{Deserialize, Serialize};
use tokio_tungstenite::tungstenite::Message;

use crate::{db::ModelAlarm, sysinfo::SysInfo};

/// Basic pi info
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PiStatus {
    pub alarm: Option<ModelAlarm>,
    pub time_zone: String,
    pub uptime_app: u64,
    pub uptime_ws: u64,
    pub uptime: usize,
    pub version: String,
}
/// Combined pi into and current set alarms
impl PiStatus {
    pub fn new(sysinfo: SysInfo, alarm: Option<ModelAlarm>, uptime_ws: u64) -> Self {
        Self {
            alarm,
            time_zone: sysinfo.time_zone,
            uptime_app: sysinfo.uptime_app,
            uptime: sysinfo.uptime,
            uptime_ws,
            version: sysinfo.version,
        }
    }
}
/// Responses, either sent as is, or nested in StructuredResponse below
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case", tag = "name", content = "data")]
pub enum Response {
    Status(PiStatus),
    LedStatus { status: bool },
    Error(String),
}

/// These get sent to the websocket server when in structured_data mode,
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct StructuredResponse {
    data: Option<Response>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Response>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    unique: Option<String>,
}

impl StructuredResponse {
    /// Convert a ResponseMessage into a Tokio message of StructureResponse
    pub fn data(data: Response, cache: Option<bool>, unique: Option<String>) -> Message {
        let x = Self {
            data: Some(data),
            error: None,
            cache,
            unique,
        };
        Message::Text(serde_json::to_string(&x).unwrap_or_default())
    }

    /// Convert a ErrorResponse into a Tokio message of StructureResponse
    pub fn _error(data: Response) -> Message {
        let x = Self {
            error: Some(data),
            data: None,
            cache: None,
            unique: None,
        };
        Message::Text(serde_json::to_string(&x).unwrap_or_default())
    }
}

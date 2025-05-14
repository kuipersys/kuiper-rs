use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum ClientMessage {
    #[serde(rename = "subscribe")]
    Subscribe { resource: String },
    #[serde(rename = "rpc")]
    Rpc { method: String, payload: Value },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    #[serde(rename = "event")]
    Event {
        resource: String,
        namespace: Option<String>,
        action: String,
        object: Value,
    },
    #[serde(rename = "pong")]
    Pong,
    #[serde(rename = "hello")]
    Hello {
        client_id: String,
        message: String,  
    },
    #[serde(rename = "error")]
    Error {
        message: String,
    },
}
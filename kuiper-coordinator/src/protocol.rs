use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Messages sent from the coordinator to the resource-server.
#[derive(Serialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Subscribe { resource: String },
}

/// Messages received from the resource-server.
#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Hello {
        client_id: String,
        message: String,
    },
    Subscribed {
        resource: String,
    },
    Event {
        resource: String,
        namespace: Option<String>,
        action: String,
        object: Value,
    },
    RpcResult {
        value: Value,
    },
    Error {
        message: String,
    },
    Pong,
}

use serde::{ Serialize, Deserialize };
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserId(Uuid);
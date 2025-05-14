use thiserror::Error;

#[derive(Error, Debug)]
pub enum CloudApiError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Invalid response structure")]
    InvalidResponse,

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
}

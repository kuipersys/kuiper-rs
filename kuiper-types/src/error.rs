use thiserror::Error;

/// Domain errors raised by the Kuiper runtime and exposed to callers.
/// These are propagated through `anyhow` and can be downcasted by HTTP
/// handlers or other layers to produce the correct status code / message.
#[derive(Error, Debug)]
pub enum KuiperError {
    /// The requested resource does not exist (or has been soft-deleted).
    #[error("Not found: {0}")]
    NotFound(String),

    /// A write was rejected because the caller's `resourceVersion` did not
    /// match the version currently stored (optimistic concurrency failure).
    #[error("Conflict: {0}")]
    Conflict(String),

    /// The request was structurally invalid.
    #[error("Invalid request: {0}")]
    Invalid(String),

    /// The operation is not permitted (e.g. reserved UID prefix used by caller).
    #[error("Forbidden: {0}")]
    Forbidden(String),

    /// A downstream service required to fulfil the request was unreachable or
    /// returned an error (e.g. an admission webhook `ServiceEndpoint` is down).
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),
}

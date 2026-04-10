use serde::{Deserialize, Serialize};

use super::resource::SystemObjectMetadata;

// ── ServiceAuth ───────────────────────────────────────────────────────────────

/// Authentication strategy used when the runtime calls a registered endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServiceAuth {
    /// No authentication — plain HTTP/HTTPS with no credentials.
    None,

    /// Mutual-TLS using the cluster's own client certificate.
    ClusterCert,

    /// Bearer token read from the named environment variable at call time.
    Bearer {
        #[serde(rename = "tokenEnv")]
        token_env: String,
    },

    /// HTTP Basic auth with username/password read from the named env vars.
    Basic {
        #[serde(rename = "usernameEnv")]
        username_env: String,
        #[serde(rename = "passwordEnv")]
        password_env: String,
    },

    /// HMAC-SHA256 request signing. The signing key is read from the named
    /// environment variable at call time.
    Hmac {
        #[serde(rename = "secretEnv")]
        secret_env: String,
        #[serde(rename = "algorithm", default = "default_hmac_algorithm")]
        algorithm: String,
    },
}

fn default_hmac_algorithm() -> String {
    "sha256".to_string()
}

// ── ServiceTls ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServiceTls {
    /// Skip TLS certificate verification. Use only in development/test.
    #[serde(rename = "insecureSkipVerify", default)]
    pub insecure_skip_verify: bool,
}

// ── ServiceEndpointSpec ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceEndpointSpec {
    /// The base URL of the service (e.g. `https://billing-service.internal:8443`).
    pub url: String,

    /// How the runtime authenticates to this service.
    #[serde(default = "default_auth")]
    pub auth: ServiceAuth,

    /// TLS settings. Omit to use system defaults.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls: Option<ServiceTls>,

    /// Request timeout in seconds. Defaults to 10.
    #[serde(rename = "timeoutSeconds", default = "default_timeout")]
    pub timeout_seconds: u32,
}

fn default_auth() -> ServiceAuth {
    ServiceAuth::None
}

fn default_timeout() -> u32 {
    10
}

// ── ServiceEndpoint ───────────────────────────────────────────────────────────

/// A named, reusable endpoint that other resources (e.g. `AdmissionPolicy`)
/// reference by name rather than embedding URLs directly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceEndpoint {
    #[serde(rename = "apiVersion")]
    pub api_version: String,

    #[serde(rename = "kind")]
    pub kind: String,

    pub metadata: SystemObjectMetadata,

    pub spec: ServiceEndpointSpec,
}

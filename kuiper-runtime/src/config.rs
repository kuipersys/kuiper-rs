pub struct KuiperConfig {
    pub store_path: String,
    /// MongoDB-compatible connection string for Azure Cosmos DB for MongoDB (vCore).
    /// When set, the DocumentDB store is used instead of the filesystem store.
    /// Set via `KUIPER_DOCUMENTDB_CONNECTION_STRING`.
    pub documentdb_connection_string: Option<String>,
    /// Target database name inside the DocumentDB cluster.
    /// Set via `KUIPER_DOCUMENTDB_DATABASE`; defaults to `"kuiper"`.
    pub documentdb_database: String,
}

impl Default for KuiperConfig {
    fn default() -> Self {
        Self::from_env()
    }
}

impl KuiperConfig {
    pub fn from_env() -> Self {
        Self {
            store_path: std::env::var("KUIPER_STORE_PATH")
                .unwrap_or_else(|_| "kuiper-store".to_string()),
            documentdb_connection_string: std::env::var("KUIPER_DOCUMENTDB_CONNECTION_STRING")
                .ok()
                .filter(|s| !s.is_empty()),
            documentdb_database: std::env::var("KUIPER_DOCUMENTDB_DATABASE")
                .unwrap_or_else(|_| "kuiper".to_string()),
        }
    }
}

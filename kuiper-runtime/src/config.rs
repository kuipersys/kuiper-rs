pub struct KuiperConfig {
    pub store_path: String,
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
        }
    }
}

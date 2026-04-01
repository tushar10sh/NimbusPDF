use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub session: SessionConfig,
    pub ai: AiConfig,
    pub storage: StorageConfig,
    pub auth: AuthConfig,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub data_dir: String,
    pub config_dir: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SessionConfig {
    pub cookie_name: String,
    pub anonymous_ttl: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AiConfig {
    pub system_prompt_file: String,
    pub summary_prompt_file: String,
    pub keypoints_prompt_file: String,
    pub max_context_tokens: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub struct StorageConfig {
    pub backend: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AuthConfig {
    pub require_auth: bool,
}

impl AppConfig {
    pub fn load() -> anyhow::Result<Self> {
        let config_dir = std::env::var("NIMBUS_CONFIG_DIR").unwrap_or_else(|_| "./config".into());
        let cfg = config::Config::builder()
            .add_source(config::File::with_name(&format!("{config_dir}/default")))
            .add_source(config::Environment::with_prefix("NIMBUS").separator("__"))
            .build()?;
        Ok(cfg.try_deserialize()?)
    }
}

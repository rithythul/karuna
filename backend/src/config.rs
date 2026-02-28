use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub redis_url: String,
    pub openrouter_api_key: String,
    pub openrouter_base_url: String,
    pub default_model: String,
    pub planning_model: String,
    pub fast_model: String,
    pub sandbox_image: String,
    pub sandbox_memory_limit: i64,
    pub sandbox_cpu_quota: i64,
    pub host: String,
    pub port: u16,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            database_url: env::var("KARUNA_DATABASE_URL")
                .unwrap_or_else(|_| "postgresql://karuna:karuna@localhost:5432/karuna".into()),
            redis_url: env::var("KARUNA_REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".into()),
            openrouter_api_key: env::var("KARUNA_OPENROUTER_API_KEY")
                .unwrap_or_default(),
            openrouter_base_url: env::var("KARUNA_OPENROUTER_BASE_URL")
                .unwrap_or_else(|_| "https://openrouter.ai/api/v1".into()),
            default_model: env::var("KARUNA_DEFAULT_MODEL")
                .unwrap_or_else(|_| "anthropic/claude-sonnet-4".into()),
            planning_model: env::var("KARUNA_PLANNING_MODEL")
                .unwrap_or_else(|_| "anthropic/claude-sonnet-4".into()),
            fast_model: env::var("KARUNA_FAST_MODEL")
                .unwrap_or_else(|_| "anthropic/claude-haiku-4".into()),
            sandbox_image: env::var("KARUNA_SANDBOX_IMAGE")
                .unwrap_or_else(|_| "karuna-sandbox:latest".into()),
            sandbox_memory_limit: env::var("KARUNA_SANDBOX_MEMORY_MB")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(512),
            sandbox_cpu_quota: env::var("KARUNA_SANDBOX_CPU_QUOTA")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(50000),
            host: env::var("KARUNA_HOST").unwrap_or_else(|_| "0.0.0.0".into()),
            port: env::var("KARUNA_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(8000),
        }
    }
}

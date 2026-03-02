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
    // KOOMPI KID OAuth
    pub koompi_client_id: String,
    pub koompi_client_secret: String,
    pub koompi_redirect_uri: String,
    pub public_url: String,
}

impl Config {
    pub fn from_env() -> Self {
        let public_url = env::var("HANUMAN_PUBLIC_URL")
            .unwrap_or_else(|_| "http://localhost:3000".into());
        let koompi_redirect_uri = env::var("HANUMAN_KOOMPI_REDIRECT_URI")
            .unwrap_or_else(|_| format!("{public_url}/auth/callback"));

        Self {
            database_url: env::var("HANUMAN_DATABASE_URL")
                .unwrap_or_else(|_| "postgresql://hanuman:hanuman@localhost:5432/hanuman".into()),
            redis_url: env::var("HANUMAN_REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".into()),
            openrouter_api_key: env::var("HANUMAN_OPENROUTER_API_KEY")
                .unwrap_or_default(),
            openrouter_base_url: env::var("HANUMAN_OPENROUTER_BASE_URL")
                .unwrap_or_else(|_| "https://openrouter.ai/api/v1".into()),
            default_model: env::var("HANUMAN_DEFAULT_MODEL")
                .unwrap_or_else(|_| "anthropic/claude-sonnet-4".into()),
            planning_model: env::var("HANUMAN_PLANNING_MODEL")
                .unwrap_or_else(|_| "anthropic/claude-sonnet-4".into()),
            fast_model: env::var("HANUMAN_FAST_MODEL")
                .unwrap_or_else(|_| "anthropic/claude-haiku-4".into()),
            sandbox_image: env::var("HANUMAN_SANDBOX_IMAGE")
                .unwrap_or_else(|_| "hanuman-sandbox:latest".into()),
            sandbox_memory_limit: env::var("HANUMAN_SANDBOX_MEMORY_MB")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(512),
            sandbox_cpu_quota: env::var("HANUMAN_SANDBOX_CPU_QUOTA")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(50000),
            host: env::var("HANUMAN_HOST").unwrap_or_else(|_| "0.0.0.0".into()),
            port: env::var("HANUMAN_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(8000),
            koompi_client_id: env::var("HANUMAN_KOOMPI_CLIENT_ID")
                .unwrap_or_default(),
            koompi_client_secret: env::var("HANUMAN_KOOMPI_CLIENT_SECRET")
                .unwrap_or_default(),
            koompi_redirect_uri,
            public_url,
        }
    }
}

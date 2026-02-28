mod config;
mod db;
mod error;
mod llm;
mod models;
mod redis_client;

use axum::{routing::get, Json, Router};
use sqlx::postgres::PgPoolOptions;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use config::Config;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub config: Config,
    pub llm: llm::LlmClient,
    pub redis: redis_client::RedisClient,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "karuna=debug,tower_http=debug".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    dotenvy::dotenv().ok();
    let config = Config::from_env();

    let db = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await
        .expect("Failed to connect to database");

    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .expect("Failed to run migrations");

    let llm = llm::LlmClient::new(&config);
    let redis = redis_client::RedisClient::new(&config.redis_url)
        .expect("Failed to create Redis connection pool");
    let state = AppState { db, config: config.clone(), llm, redis };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", get(health))
        .layer(cors)
        .with_state(state);

    let addr = format!("{}:{}", config.host, config.port);
    tracing::info!("Karuna listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}

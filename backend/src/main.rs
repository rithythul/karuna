mod api;
mod auth;
mod config;
mod db;
mod error;
mod llm;
mod models;
mod orchestrator;
mod redis_client;
mod sandbox;
pub mod soul;
mod tools;
mod agent_runtime;
mod agents;
mod ws;

use std::sync::Arc;

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
    pub sandbox: sandbox::SandboxManager,
    pub agents: Arc<agent_runtime::AgentRegistry>,
    pub orchestrator: orchestrator::Orchestrator,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "karuna=debug,tower_http=debug".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    dotenvy::dotenv().ok();
    soul::load("soul.md");
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
    let sandbox = sandbox::SandboxManager::new(&config)
        .expect("Failed to connect to Docker");
    // Don't warm pool on startup for dev — it requires the sandbox image to be built
    // sandbox.warm_pool(3).await.expect("Failed to warm sandbox pool");
    let agents = Arc::new(agents::default_registry());

    let orchestrator = orchestrator::Orchestrator::new(
        db.clone(), llm.clone(), sandbox.clone(), agents.clone(), redis.clone(),
    );

    // Spawn worker in background
    let worker = orchestrator.clone();
    tokio::spawn(async move { worker.run_worker().await });

    let state = AppState { db, config: config.clone(), llm, redis, sandbox, agents, orchestrator };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", get(health))
        .route("/ws/tasks/{id}", get(ws::ws_handler))
        .merge(auth::routes())
        .merge(api::routes())
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

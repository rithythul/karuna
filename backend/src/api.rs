use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::db;
use crate::error::AppError;
use crate::models::{CreateTaskRequest, CreateTaskResponse};
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/tasks", post(create_task).get(list_tasks))
        .route("/api/tasks/{id}", get(get_task))
        .route("/api/tasks/{id}/events", get(get_task_events))
        .route("/api/tasks/{id}/artifacts", get(get_task_artifacts))
        .route("/api/skills", get(list_skills))
        .route("/api/status", get(system_status))
}

async fn create_task(
    State(state): State<AppState>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<Json<CreateTaskResponse>, AppError> {
    let task = db::create_task(&state.db, "default-user", &req.goal).await?;

    // Enqueue to Redis job queue
    state.orchestrator.enqueue(task.id).await?;

    Ok(Json(CreateTaskResponse { task_id: task.id }))
}

async fn list_tasks(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let tasks = sqlx::query_as::<_, crate::models::Task>(
        "SELECT * FROM tasks ORDER BY created_at DESC LIMIT 50"
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(serde_json::json!({"tasks": tasks})))
}

async fn get_task(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let task = db::get_task(&state.db, id).await?;
    let steps = db::get_task_steps(&state.db, id).await?;
    let artifacts = db::get_task_artifacts(&state.db, id).await?;
    Ok(Json(serde_json::json!({
        "task": task,
        "steps": steps,
        "artifacts": artifacts,
    })))
}

async fn get_task_events(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<crate::models::TaskEvent>>, AppError> {
    let events = db::get_task_events(&state.db, id).await?;
    Ok(Json(events))
}

async fn get_task_artifacts(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<crate::models::Artifact>>, AppError> {
    let artifacts = db::get_task_artifacts(&state.db, id).await?;
    Ok(Json(artifacts))
}

async fn list_skills(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let skills: Vec<_> = state.skills.list()
        .into_iter()
        .map(|(name, desc)| serde_json::json!({"name": name, "description": desc}))
        .collect();
    Json(serde_json::json!({"skills": skills}))
}

async fn system_status(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let queue_len = state.redis.queue_length().await.unwrap_or(0);
    let sandbox_pool = state.sandbox.pool_size().await;
    let skills: Vec<_> = state.skills.list().into_iter().map(|(n, _)| n.to_string()).collect();
    Json(serde_json::json!({
        "status": "ok",
        "queue_length": queue_len,
        "sandbox_pool_size": sandbox_pool,
        "available_skills": skills,
    }))
}

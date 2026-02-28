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
        .route("/api/tasks", post(create_task))
        .route("/api/tasks/{id}", get(get_task))
        .route("/api/tasks/{id}/events", get(get_task_events))
        .route("/api/skills", get(list_skills))
}

async fn create_task(
    State(state): State<AppState>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<Json<CreateTaskResponse>, AppError> {
    let task = db::create_task(&state.db, "default-user", &req.goal).await?;

    // Enqueue to Redis job queue (not tokio::spawn)
    state.orchestrator.enqueue(task.id).await?;

    Ok(Json(CreateTaskResponse { task_id: task.id }))
}

async fn get_task(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let task = db::get_task(&state.db, id).await?;
    let steps = db::get_task_steps(&state.db, id).await?;
    Ok(Json(serde_json::json!({
        "task": task,
        "steps": steps,
    })))
}

async fn get_task_events(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<crate::models::TaskEvent>>, AppError> {
    let events = db::get_task_events(&state.db, id).await?;
    Ok(Json(events))
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

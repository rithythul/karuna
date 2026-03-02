use axum::{
    extract::{Path, State},
    routing::{delete, get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::auth::AuthUser;
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
        .route("/api/tasks/{id}/cancel", post(cancel_task))
        .route("/api/tasks/{id}/steps/{step_id}/reasoning", get(get_step_reasoning))
        .route("/api/memory", get(list_user_memories))
        .route("/api/memory/{category}", get(list_user_memories_by_category))
        .route("/api/memory/{category}/{key}", delete(delete_user_memory))
        .route("/api/skills", get(list_skills))
        .route("/api/status", get(system_status))
}

async fn create_task(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<Json<CreateTaskResponse>, AppError> {
    let task = db::create_task(&state.db, &auth.0.id, &req.goal).await?;

    // Enqueue to Redis job queue
    state.orchestrator.enqueue(task.id).await?;

    Ok(Json(CreateTaskResponse { task_id: task.id }))
}

async fn list_tasks(
    _auth: AuthUser,
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
    _auth: AuthUser,
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
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<crate::models::TaskEvent>>, AppError> {
    let events = db::get_task_events(&state.db, id).await?;
    Ok(Json(events))
}

async fn get_task_artifacts(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<crate::models::Artifact>>, AppError> {
    let artifacts = db::get_task_artifacts(&state.db, id).await?;
    Ok(Json(artifacts))
}

async fn get_step_reasoning(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path((task_id, step_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<crate::models::ReasoningTrace>>, AppError> {
    let traces = db::get_reasoning_traces(&state.db, task_id, Some(step_id)).await?;
    Ok(Json(traces))
}

async fn cancel_task(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.orchestrator.cancel_task(id).await?;
    Ok(Json(serde_json::json!({"status": "cancelled"})))
}

async fn list_skills(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let agents: Vec<_> = state.agents.list()
        .into_iter()
        .map(|(name, desc)| serde_json::json!({"name": name, "description": desc}))
        .collect();
    Json(serde_json::json!({"skills": agents}))
}

async fn system_status(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let queue_len = state.redis.queue_length().await.unwrap_or(0);
    let sandbox_pool = state.sandbox.pool_size().await;
    let agents: Vec<_> = state.agents.list().into_iter().map(|(n, _)| n.to_string()).collect();
    Json(serde_json::json!({
        "status": "ok",
        "queue_length": queue_len,
        "sandbox_pool_size": sandbox_pool,
        "available_skills": agents,
    }))
}

// --- User Memory endpoints ---

async fn list_user_memories(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let memories = db::get_all_user_memories(&state.db, &auth.0.id, 200).await?;
    Ok(Json(serde_json::json!({"memories": memories})))
}

async fn list_user_memories_by_category(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(category): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let memories = db::get_user_memories_by_category(&state.db, &auth.0.id, &category, 200).await?;
    Ok(Json(serde_json::json!({"memories": memories})))
}

async fn delete_user_memory(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((category, key)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, AppError> {
    let deleted = db::delete_user_memory(&state.db, &auth.0.id, &category, &key).await?;
    if deleted {
        Ok(Json(serde_json::json!({"status": "deleted"})))
    } else {
        Err(AppError::NotFound(format!("Memory {category}/{key} not found")))
    }
}

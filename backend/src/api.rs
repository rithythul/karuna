use axum::{
    extract::{FromRequest, Multipart, Path, State},
    http::{header, HeaderMap},
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::db;
use crate::error::AppError;
use crate::models::{CreateTaskRequest, CreateTaskResponse, Task};
use crate::AppState;

/// Fetch a task and verify it belongs to the authenticated user.
async fn get_user_task(state: &AppState, user_id: &str, task_id: Uuid) -> Result<Task, AppError> {
    let task = db::get_task(&state.db, task_id).await?;
    if task.user_id != user_id {
        return Err(AppError::NotFound(format!("Task {task_id} not found")));
    }
    Ok(task)
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/tasks", post(create_task).get(list_tasks))
        .route("/api/tasks/{id}", get(get_task))
        .route("/api/tasks/{id}/events", get(get_task_events))
        .route("/api/tasks/{id}/artifacts", get(get_task_artifacts))
        .route("/api/tasks/{task_id}/artifacts/{artifact_id}/content", get(get_artifact_content))
        .route("/api/tasks/{id}/cancel", post(cancel_task))
        .route("/api/tasks/{id}/steps/{step_id}/reasoning", get(get_step_reasoning))
        .route("/api/memory", get(list_user_memories))
        .route("/api/memory/{category}", get(list_user_memories_by_category))
        .route("/api/memory/{category}/{key}", delete(delete_user_memory))
        .route("/api/skills", get(list_skills))
        .route("/api/status", get(system_status))
}

/// Shared task creation logic used by both the JSON and multipart paths.
async fn create_task_core(
    state: &AppState,
    user_id: &str,
    goal: &str,
) -> Result<CreateTaskResponse, AppError> {
    let task = db::create_task(&state.db, user_id, goal).await?;

    // Enqueue to Redis job queue
    state.orchestrator.enqueue(task.id).await?;

    Ok(CreateTaskResponse { task_id: task.id })
}

async fn create_task(
    auth: AuthUser,
    State(state): State<AppState>,
    request: axum::extract::Request,
) -> Result<Json<CreateTaskResponse>, AppError> {
    let content_type = request
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if content_type.starts_with("multipart/form-data") {
        let multipart = Multipart::from_request(request, &state)
            .await
            .map_err(|e| AppError::BadRequest(format!("Multipart error: {e}")))?;
        handle_multipart_task(state, auth, multipart).await
    } else {
        let body_bytes = axum::body::to_bytes(request.into_body(), 1024 * 1024)
            .await
            .map_err(|e| AppError::BadRequest(format!("Failed to read body: {e}")))?;
        let req: CreateTaskRequest = serde_json::from_slice(&body_bytes)
            .map_err(|e| AppError::BadRequest(format!("Invalid JSON: {e}")))?;
        let resp = create_task_core(&state, &auth.0.id, &req.goal).await?;
        Ok(Json(resp))
    }
}

async fn handle_multipart_task(
    state: AppState,
    auth: AuthUser,
    mut multipart: Multipart,
) -> Result<Json<CreateTaskResponse>, AppError> {
    let mut goal: Option<String> = None;
    let mut files: Vec<(String, String, String)> = Vec::new(); // (name, mime_type, base64_content)

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Multipart error: {e}")))?
    {
        let field_name = field.name().unwrap_or("").to_string();

        if field_name == "goal" {
            goal = Some(
                field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read goal: {e}")))?,
            );
        } else if field.file_name().is_some() {
            // File field — validate count and size
            if files.len() >= 5 {
                return Err(AppError::BadRequest("Maximum 5 files allowed".into()));
            }

            let filename = field.file_name().unwrap_or("file").to_string();
            let mime_type = field
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_string();

            const MAX_FILE_SIZE: usize = 10 * 1024 * 1024; // 10 MB
            let mut buf = Vec::with_capacity(64 * 1024);
            while let Some(chunk) = field.chunk().await
                .map_err(|e| AppError::BadRequest(format!("Failed to read file: {e}")))?
            {
                if buf.len() + chunk.len() > MAX_FILE_SIZE {
                    return Err(AppError::BadRequest(
                        format!("File '{}' exceeds 10 MB limit", filename)
                    ));
                }
                buf.extend_from_slice(&chunk);
            }
            let content = BASE64.encode(&buf);
            files.push((filename, mime_type, content));
        } else {
            // Unknown text field — skip it silently
        }
    }

    let goal = goal.ok_or_else(|| AppError::BadRequest("Missing 'goal' field".into()))?;

    // Use a transaction so that the task row and all artifact rows are either
    // all committed or all rolled back — preventing permanently-orphaned tasks.
    let mut tx = state.db.begin().await
        .map_err(|e| AppError::Internal(format!("Failed to start transaction: {e}")))?;

    let task_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO tasks (user_id, goal) VALUES ($1, $2) RETURNING id"
    )
    .bind(&auth.0.id)
    .bind(&goal)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(format!("Failed to create task: {e}")))?;

    for (name, mime_type, content) in &files {
        sqlx::query(
            "INSERT INTO artifacts (task_id, name, artifact_type, mime_type, content) \
             VALUES ($1, $2, 'input', $3, $4)"
        )
        .bind(task_id)
        .bind(name)
        .bind(mime_type)
        .bind(content)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to store artifact: {e}")))?;
    }

    tx.commit().await
        .map_err(|e| AppError::Internal(format!("Failed to commit transaction: {e}")))?;

    // Only enqueue to Redis after all DB writes have succeeded
    state.orchestrator.enqueue(task_id).await?;

    Ok(Json(CreateTaskResponse { task_id }))
}

async fn list_tasks(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let tasks = sqlx::query_as::<_, crate::models::Task>(
        "SELECT * FROM tasks WHERE user_id = $1 ORDER BY created_at DESC LIMIT 50"
    )
    .bind(&auth.0.id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(serde_json::json!({"tasks": tasks})))
}

async fn get_task(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let task = get_user_task(&state, &auth.0.id, id).await?;
    let steps = db::get_task_steps(&state.db, id).await?;
    let artifacts = db::get_task_artifacts(&state.db, id).await?;
    Ok(Json(serde_json::json!({
        "task": task,
        "steps": steps,
        "artifacts": artifacts,
    })))
}

async fn get_task_events(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<crate::models::TaskEvent>>, AppError> {
    get_user_task(&state, &auth.0.id, id).await?;
    let events = db::get_task_events(&state.db, id).await?;
    Ok(Json(events))
}

async fn get_task_artifacts(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<crate::models::Artifact>>, AppError> {
    get_user_task(&state, &auth.0.id, id).await?;
    let artifacts = db::get_task_artifacts(&state.db, id).await?;
    Ok(Json(artifacts))
}

async fn get_artifact_content(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((task_id, artifact_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    get_user_task(&state, &auth.0.id, task_id).await?;
    let artifact = db::get_artifact(&state.db, artifact_id).await?;

    // Verify the artifact belongs to the requested task
    if artifact.task_id != task_id {
        return Err(AppError::NotFound(format!("Artifact {artifact_id} not found for task {task_id}")));
    }

    let content = match artifact.content {
        Some(c) => c,
        None => {
            return Err(AppError::NotFound(
                "Artifact content not available (sandbox released)".to_string(),
            ));
        }
    };

    let mime = artifact
        .mime_type
        .unwrap_or_else(|| "application/octet-stream".to_string());

    // Determine Content-Disposition: inline for viewable types, attachment for others
    let is_viewable = mime.starts_with("text/")
        || mime.starts_with("image/")
        || mime == "application/json";
    let disposition = if is_viewable {
        format!("inline; filename=\"{}\"", artifact.name)
    } else {
        format!("attachment; filename=\"{}\"", artifact.name)
    };

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        mime.parse().unwrap_or_else(|_| "application/octet-stream".parse().unwrap()),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        disposition.parse().unwrap_or_else(|_| "attachment".parse().unwrap()),
    );

    Ok((headers, content))
}

async fn get_step_reasoning(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((task_id, step_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<crate::models::ReasoningTrace>>, AppError> {
    get_user_task(&state, &auth.0.id, task_id).await?;
    let traces = db::get_reasoning_traces(&state.db, task_id, Some(step_id)).await?;
    Ok(Json(traces))
}

async fn cancel_task(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    get_user_task(&state, &auth.0.id, id).await?;
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

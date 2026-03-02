use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppError;
use crate::models::{Artifact, ReasoningTrace, Task, TaskEvent, TaskStatus, TaskStep, UserMemory};

pub async fn create_task(pool: &PgPool, user_id: &str, goal: &str) -> Result<Task, AppError> {
    let task = sqlx::query_as::<_, Task>(
        "INSERT INTO tasks (user_id, goal) VALUES ($1, $2) RETURNING *"
    )
    .bind(user_id)
    .bind(goal)
    .fetch_one(pool)
    .await?;
    Ok(task)
}

pub async fn get_task(pool: &PgPool, task_id: Uuid) -> Result<Task, AppError> {
    sqlx::query_as::<_, Task>("SELECT * FROM tasks WHERE id = $1")
        .bind(task_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Task {task_id} not found")))
}

pub async fn update_task_status(
    pool: &PgPool,
    task_id: Uuid,
    status: TaskStatus,
) -> Result<Task, AppError> {
    let task = sqlx::query_as::<_, Task>(
        "UPDATE tasks SET status = $1, updated_at = now() WHERE id = $2 RETURNING *"
    )
    .bind(&status)
    .bind(task_id)
    .fetch_one(pool)
    .await?;
    Ok(task)
}

pub async fn set_task_plan(
    pool: &PgPool,
    task_id: Uuid,
    plan: serde_json::Value,
) -> Result<(), AppError> {
    sqlx::query("UPDATE tasks SET plan = $1, updated_at = now() WHERE id = $2")
        .bind(&plan)
        .bind(task_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_task_result(
    pool: &PgPool,
    task_id: Uuid,
    result: serde_json::Value,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE tasks SET result = $1, status = 'completed', updated_at = now() WHERE id = $2"
    )
    .bind(&result)
    .bind(task_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_task_error(
    pool: &PgPool,
    task_id: Uuid,
    error: &str,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE tasks SET error = $1, status = 'failed', updated_at = now() WHERE id = $2"
    )
    .bind(error)
    .bind(task_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn create_task_step(
    pool: &PgPool,
    task_id: Uuid,
    skill: &str,
    description: &str,
    step_order: i32,
) -> Result<TaskStep, AppError> {
    let step = sqlx::query_as::<_, TaskStep>(
        "INSERT INTO task_steps (task_id, skill, description, step_order) \
         VALUES ($1, $2, $3, $4) RETURNING *"
    )
    .bind(task_id)
    .bind(skill)
    .bind(description)
    .bind(step_order)
    .fetch_one(pool)
    .await?;
    Ok(step)
}

pub async fn get_task_steps(pool: &PgPool, task_id: Uuid) -> Result<Vec<TaskStep>, AppError> {
    let steps = sqlx::query_as::<_, TaskStep>(
        "SELECT * FROM task_steps WHERE task_id = $1 ORDER BY step_order"
    )
    .bind(task_id)
    .fetch_all(pool)
    .await?;
    Ok(steps)
}

pub async fn update_step_status(
    pool: &PgPool,
    step_id: Uuid,
    status: TaskStatus,
) -> Result<(), AppError> {
    sqlx::query("UPDATE task_steps SET status = $1 WHERE id = $2")
        .bind(&status)
        .bind(step_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn add_task_event(
    pool: &PgPool,
    task_id: Uuid,
    event_type: &str,
    data: serde_json::Value,
) -> Result<TaskEvent, AppError> {
    let event = sqlx::query_as::<_, TaskEvent>(
        "INSERT INTO task_events (task_id, event_type, data) VALUES ($1, $2, $3) RETURNING *"
    )
    .bind(task_id)
    .bind(event_type)
    .bind(&data)
    .fetch_one(pool)
    .await?;
    Ok(event)
}

pub async fn get_task_events(pool: &PgPool, task_id: Uuid) -> Result<Vec<TaskEvent>, AppError> {
    let events = sqlx::query_as::<_, TaskEvent>(
        "SELECT * FROM task_events WHERE task_id = $1 ORDER BY created_at"
    )
    .bind(task_id)
    .fetch_all(pool)
    .await?;
    Ok(events)
}

// --- Artifacts ---

pub async fn create_artifact(
    pool: &PgPool,
    task_id: Uuid,
    step_id: Option<Uuid>,
    name: &str,
    artifact_type: &str,
    mime_type: Option<&str>,
    path_in_sandbox: Option<&str>,
    content: Option<&str>,
    metadata: Option<serde_json::Value>,
    size_bytes: Option<i64>,
) -> Result<Artifact, AppError> {
    let artifact = sqlx::query_as::<_, Artifact>(
        "INSERT INTO artifacts (task_id, step_id, name, artifact_type, mime_type, \
         path_in_sandbox, content, metadata, size_bytes) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING *"
    )
    .bind(task_id)
    .bind(step_id)
    .bind(name)
    .bind(artifact_type)
    .bind(mime_type)
    .bind(path_in_sandbox)
    .bind(content)
    .bind(&metadata)
    .bind(size_bytes)
    .fetch_one(pool)
    .await?;
    Ok(artifact)
}

pub async fn get_artifact(pool: &PgPool, artifact_id: Uuid) -> Result<Artifact, AppError> {
    sqlx::query_as::<_, Artifact>("SELECT * FROM artifacts WHERE id = $1")
        .bind(artifact_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Artifact {artifact_id} not found")))
}

pub async fn get_task_artifacts(pool: &PgPool, task_id: Uuid) -> Result<Vec<Artifact>, AppError> {
    let artifacts = sqlx::query_as::<_, Artifact>(
        "SELECT * FROM artifacts WHERE task_id = $1 ORDER BY created_at"
    )
    .bind(task_id)
    .fetch_all(pool)
    .await?;
    Ok(artifacts)
}

// --- Task Memory ---

pub async fn set_memory(
    pool: &PgPool,
    task_id: Uuid,
    key: &str,
    value: serde_json::Value,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO task_memory (task_id, key, value) VALUES ($1, $2, $3) \
         ON CONFLICT (task_id, key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()"
    )
    .bind(task_id)
    .bind(key)
    .bind(&value)
    .execute(pool)
    .await?;
    Ok(())
}

// --- User Memory (cross-task) ---

pub async fn set_user_memory(
    pool: &PgPool,
    user_id: &str,
    category: &str,
    key: &str,
    value: serde_json::Value,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO user_memory (user_id, category, key, value) VALUES ($1, $2, $3, $4) \
         ON CONFLICT (user_id, category, key) DO UPDATE SET \
         value = EXCLUDED.value, access_count = user_memory.access_count + 1, updated_at = now()"
    )
    .bind(user_id)
    .bind(category)
    .bind(key)
    .bind(&value)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_user_memories_by_category(
    pool: &PgPool,
    user_id: &str,
    category: &str,
    limit: i64,
) -> Result<Vec<UserMemory>, AppError> {
    let mems = sqlx::query_as::<_, UserMemory>(
        "SELECT * FROM user_memory WHERE user_id = $1 AND category = $2 \
         ORDER BY access_count DESC, updated_at DESC LIMIT $3"
    )
    .bind(user_id)
    .bind(category)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(mems)
}

pub async fn get_all_user_memories(
    pool: &PgPool,
    user_id: &str,
    limit: i64,
) -> Result<Vec<UserMemory>, AppError> {
    let mems = sqlx::query_as::<_, UserMemory>(
        "SELECT * FROM user_memory WHERE user_id = $1 \
         ORDER BY updated_at DESC LIMIT $2"
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(mems)
}

pub async fn delete_user_memory(
    pool: &PgPool,
    user_id: &str,
    category: &str,
    key: &str,
) -> Result<bool, AppError> {
    let result = sqlx::query(
        "DELETE FROM user_memory WHERE user_id = $1 AND category = $2 AND key = $3"
    )
    .bind(user_id)
    .bind(category)
    .bind(key)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn prune_user_memories(
    pool: &PgPool,
    user_id: &str,
    max_entries: i64,
) -> Result<u64, AppError> {
    let result = sqlx::query(
        "DELETE FROM user_memory WHERE id IN (\
           SELECT id FROM user_memory WHERE user_id = $1 \
           ORDER BY access_count DESC, updated_at DESC \
           OFFSET $2\
         )"
    )
    .bind(user_id)
    .bind(max_entries)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

// --- Task duration ---

pub async fn set_task_duration(
    pool: &PgPool,
    task_id: Uuid,
    duration_ms: i64,
) -> Result<(), AppError> {
    sqlx::query("UPDATE tasks SET total_duration_ms = $1, updated_at = now() WHERE id = $2")
        .bind(duration_ms)
        .bind(task_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_task_token_usage(
    pool: &PgPool,
    task_id: Uuid,
    usage: serde_json::Value,
) -> Result<(), AppError> {
    sqlx::query("UPDATE tasks SET token_usage = $1, updated_at = now() WHERE id = $2")
        .bind(&usage)
        .bind(task_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn increment_step_retry(
    pool: &PgPool,
    step_id: Uuid,
) -> Result<i32, AppError> {
    let row = sqlx::query_scalar::<_, i32>(
        "UPDATE task_steps SET retry_count = retry_count + 1 WHERE id = $1 RETURNING retry_count"
    )
    .bind(step_id)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn set_step_reflection(
    pool: &PgPool,
    step_id: Uuid,
    reflection: &str,
) -> Result<(), AppError> {
    sqlx::query("UPDATE task_steps SET reflection = $1 WHERE id = $2")
        .bind(reflection)
        .bind(step_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_step_error(
    pool: &PgPool,
    step_id: Uuid,
    error: &str,
) -> Result<(), AppError> {
    sqlx::query("UPDATE task_steps SET error = $1 WHERE id = $2")
        .bind(error)
        .bind(step_id)
        .execute(pool)
        .await?;
    Ok(())
}

// --- Reasoning Traces ---

pub async fn insert_reasoning_trace(
    pool: &PgPool,
    task_id: Uuid,
    step_id: Option<Uuid>,
    agent_name: &str,
    turn: i32,
    role: &str,
    content: Option<&str>,
    tool_calls: Option<serde_json::Value>,
) -> Result<ReasoningTrace, AppError> {
    let trace = sqlx::query_as::<_, ReasoningTrace>(
        "INSERT INTO reasoning_traces (task_id, step_id, agent_name, turn, role, content, tool_calls) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING *"
    )
    .bind(task_id)
    .bind(step_id)
    .bind(agent_name)
    .bind(turn)
    .bind(role)
    .bind(content)
    .bind(&tool_calls)
    .fetch_one(pool)
    .await?;
    Ok(trace)
}

pub async fn get_reasoning_traces(
    pool: &PgPool,
    task_id: Uuid,
    step_id: Option<Uuid>,
) -> Result<Vec<ReasoningTrace>, AppError> {
    let traces = match step_id {
        Some(sid) => {
            sqlx::query_as::<_, ReasoningTrace>(
                "SELECT * FROM reasoning_traces WHERE task_id = $1 AND step_id = $2 ORDER BY turn"
            )
            .bind(task_id)
            .bind(sid)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as::<_, ReasoningTrace>(
                "SELECT * FROM reasoning_traces WHERE task_id = $1 ORDER BY turn"
            )
            .bind(task_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(traces)
}

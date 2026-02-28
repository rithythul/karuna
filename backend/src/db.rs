use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppError;
use crate::models::{Task, TaskEvent, TaskStatus, TaskStep};

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

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Type;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[sqlx(type_name = "task_status", rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Planning,
    Running,
    Paused,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Task {
    pub id: Uuid,
    pub user_id: String,
    pub goal: String,
    pub status: TaskStatus,
    pub plan: Option<serde_json::Value>,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub sandbox_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub token_usage: Option<serde_json::Value>,
    pub total_duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TaskStep {
    pub id: Uuid,
    pub task_id: Uuid,
    pub skill: String,
    pub description: String,
    pub step_order: i32,
    pub status: TaskStatus,
    pub input_data: Option<serde_json::Value>,
    pub output_data: Option<serde_json::Value>,
    pub error: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub retry_count: i32,
    pub reflection: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TaskEvent {
    pub id: Uuid,
    pub task_id: Uuid,
    pub event_type: String,
    pub data: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// An artifact produced by a skill (file, screenshot, report, etc.)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Artifact {
    pub id: Uuid,
    pub task_id: Uuid,
    pub step_id: Option<Uuid>,
    pub name: String,
    pub artifact_type: String,
    pub mime_type: Option<String>,
    pub path_in_sandbox: Option<String>,
    pub content: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub size_bytes: Option<i64>,
    pub created_at: DateTime<Utc>,
}

/// Persistent memory entry for a task (context across steps)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TaskMemory {
    pub id: Uuid,
    pub task_id: Uuid,
    pub key: String,
    pub value: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    pub goal: String,
}

#[derive(Debug, Serialize)]
pub struct CreateTaskResponse {
    pub task_id: Uuid,
}

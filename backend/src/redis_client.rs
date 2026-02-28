use deadpool_redis::{Config as RedisConfig, Connection, Pool, Runtime};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::error::AppError;

/// Task event published via Redis Pub/Sub.
///
/// Each event belongs to a task and carries an event type (e.g. "status_changed",
/// "output", "completed") plus an arbitrary JSON payload.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskEvent {
    pub task_id: String,
    pub event_type: String,
    pub data: serde_json::Value,
}

/// Redis-backed infrastructure for Karuna.
///
/// Provides three capabilities:
/// - **Connection pool**: shared across the app via `conn()`
/// - **Pub/Sub events**: publish/subscribe to per-task event channels
/// - **Job queue**: FIFO task queue via Redis LIST (RPUSH / BLPOP)
#[derive(Clone)]
pub struct RedisClient {
    pool: Pool,
    /// Stored separately so we can create dedicated pub/sub connections
    /// (pub/sub connections cannot be pooled).
    redis_url: String,
}

impl RedisClient {
    /// Create a new RedisClient with a connection pool.
    pub fn new(redis_url: &str) -> Result<Self, AppError> {
        let cfg = RedisConfig::from_url(redis_url);
        let pool = cfg
            .create_pool(Some(Runtime::Tokio1))
            .map_err(|e| AppError::Internal(format!("Redis pool error: {e}")))?;
        Ok(Self {
            pool,
            redis_url: redis_url.to_string(),
        })
    }

    /// Get a connection from the pool.
    pub async fn conn(&self) -> Result<Connection, AppError> {
        self.pool
            .get()
            .await
            .map_err(|e| AppError::Internal(format!("Redis connection error: {e}")))
    }

    // ── Pub/Sub: Event Publishing ────────────────────────────

    /// Publish a task event to a Redis channel.
    ///
    /// Channel format: `karuna:task:{task_id}:events`
    pub async fn publish_event(&self, event: &TaskEvent) -> Result<(), AppError> {
        let channel = format!("karuna:task:{}:events", event.task_id);
        let payload = serde_json::to_string(event)
            .map_err(|e| AppError::Internal(format!("Serialize error: {e}")))?;
        let mut conn = self.conn().await?;
        redis::cmd("PUBLISH")
            .arg(&channel)
            .arg(&payload)
            .query_async::<()>(&mut *conn)
            .await
            .map_err(|e| AppError::Internal(format!("Redis publish error: {e}")))?;
        Ok(())
    }

    /// Subscribe to events for a specific task.
    ///
    /// Returns an mpsc receiver that yields `TaskEvent`s. A background tokio task
    /// listens on the Redis Pub/Sub channel `karuna:task:{task_id}:events` and
    /// forwards deserialized events into the channel. The background task exits
    /// when the receiver is dropped.
    ///
    /// Note: Pub/Sub requires a dedicated connection (not from the pool).
    pub async fn subscribe_task_events(
        &self,
        task_id: &str,
    ) -> Result<mpsc::UnboundedReceiver<TaskEvent>, AppError> {
        let channel = format!("karuna:task:{task_id}:events");
        let (tx, rx) = mpsc::unbounded_channel();

        // Pub/Sub requires a dedicated connection — create one directly.
        let client = redis::Client::open(self.redis_url.as_str())
            .map_err(|e| AppError::Internal(format!("Redis client error: {e}")))?;
        let mut pubsub = client
            .get_async_pubsub()
            .await
            .map_err(|e| AppError::Internal(format!("Redis pubsub connection error: {e}")))?;
        pubsub
            .subscribe(&channel)
            .await
            .map_err(|e| AppError::Internal(format!("Redis subscribe error: {e}")))?;

        // Consume the PubSub into an owned stream so we can move it into the task.
        let mut stream = pubsub.into_on_message();

        tokio::spawn(async move {
            use futures_util::StreamExt;
            while let Some(msg) = stream.next().await {
                let payload: String = match msg.get_payload() {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                let event: TaskEvent = match serde_json::from_str(&payload) {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                if tx.send(event).is_err() {
                    break; // receiver dropped
                }
            }
        });

        Ok(rx)
    }

    // ── Job Queue ────────────────────────────────────────────

    /// Enqueue a task ID for execution.
    ///
    /// Uses a Redis LIST as a simple FIFO queue: items are pushed to the right
    /// (`RPUSH`) and popped from the left (`BLPOP`).
    pub async fn enqueue_task(&self, task_id: &str) -> Result<(), AppError> {
        let mut conn = self.conn().await?;
        conn.rpush::<_, _, ()>("karuna:queue:tasks", task_id)
            .await
            .map_err(|e| AppError::Internal(format!("Redis enqueue error: {e}")))?;
        Ok(())
    }

    /// Dequeue a task ID for execution (blocking pop with timeout).
    ///
    /// Returns `None` if the timeout elapses with no task available.
    pub async fn dequeue_task(&self, timeout_secs: f64) -> Result<Option<String>, AppError> {
        // Use a dedicated connection for BLPOP since it blocks and can tie up
        // pooled connections, causing pool exhaustion/timeouts.
        let client = redis::Client::open(self.redis_url.as_str())
            .map_err(|e| AppError::Internal(format!("Redis client error: {e}")))?;
        let mut conn = client.get_multiplexed_async_connection().await
            .map_err(|e| AppError::Internal(format!("Redis connection error: {e}")))?;
        let result: redis::RedisResult<Option<(String, String)>> = redis::cmd("BLPOP")
            .arg("karuna:queue:tasks")
            .arg(timeout_secs)
            .query_async(&mut conn)
            .await;
        match result {
            Ok(Some((_, task_id))) => Ok(Some(task_id)),
            Ok(None) | Err(_) => Ok(None), // timeout or type mismatch = no task
        }
    }

    /// Get the current queue length.
    pub async fn queue_length(&self) -> Result<usize, AppError> {
        let mut conn = self.conn().await?;
        let len: usize = conn
            .llen("karuna:queue:tasks")
            .await
            .map_err(|e| AppError::Internal(format!("Redis queue length error: {e}")))?;
        Ok(len)
    }
}

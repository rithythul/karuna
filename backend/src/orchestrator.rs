use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use crate::db;
use crate::error::AppError;
use crate::llm::{ChatMessage, LlmClient};
use crate::models::TaskStatus;
use crate::redis_client::{RedisClient, TaskEvent};
use crate::sandbox::SandboxManager;
use crate::skills::{SkillContext, SkillRegistry};

#[derive(Clone)]
pub struct Orchestrator {
    pool: PgPool,
    llm: LlmClient,
    sandbox: SandboxManager,
    skills: Arc<SkillRegistry>,
    redis: RedisClient,
}

impl Orchestrator {
    pub fn new(
        pool: PgPool,
        llm: LlmClient,
        sandbox: SandboxManager,
        skills: Arc<SkillRegistry>,
        redis: RedisClient,
    ) -> Self {
        Self { pool, llm, sandbox, skills, redis }
    }

    /// Enqueue a task for execution via Redis
    pub async fn enqueue(&self, task_id: Uuid) -> Result<(), AppError> {
        self.redis.enqueue_task(&task_id.to_string()).await
    }

    /// Run the worker loop — pulls tasks from Redis queue and executes them.
    /// Call this in a tokio::spawn to run in background.
    pub async fn run_worker(self) {
        tracing::info!("Orchestrator worker started");
        loop {
            match self.redis.dequeue_task(5.0).await {
                Ok(Some(task_id_str)) => {
                    match Uuid::parse_str(&task_id_str) {
                        Ok(task_id) => {
                            tracing::info!("Worker picked up task {task_id}");
                            if let Err(e) = self.execute_task(task_id).await {
                                tracing::error!("Task {task_id} failed: {e}");
                            }
                        }
                        Err(e) => tracing::error!("Invalid task ID in queue: {e}"),
                    }
                }
                Ok(None) => {} // timeout, loop again
                Err(e) => {
                    tracing::error!("Queue dequeue error: {e}");
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }
    }

    /// Execute a single task end-to-end
    async fn execute_task(&self, task_id: Uuid) -> Result<(), AppError> {
        let task = db::get_task(&self.pool, task_id).await?;
        self.emit(task_id, "task_started", json!({"goal": task.goal})).await;

        // Phase 1: Plan
        db::update_task_status(&self.pool, task_id, TaskStatus::Planning).await?;
        self.emit(task_id, "planning", json!({})).await;

        let skill_list = self.skills.list_with_schema();
        let skill_descriptions = skill_list.iter()
            .map(|(name, desc, schema)| format!("- {name}: {desc}\n  Input schema: {schema}"))
            .collect::<Vec<_>>()
            .join("\n");

        let plan_prompt = format!(
            "You are a task planner. Decompose this goal into sequential steps.\n\n\
             Goal: {goal}\n\n\
             Available skills:\n{skill_descriptions}\n\n\
             Return a JSON array of steps. Each step has:\n\
             - \"skill\": the skill name (must be one of the available skills above)\n\
             - \"description\": what this step does\n\
             - \"input\": the input object matching the skill's input schema EXACTLY\n\n\
             IMPORTANT: The \"input\" field must use the EXACT field names from the skill's input schema.\n\
             For the \"code\" skill, use {{\"task\": \"description of what to code\", \"language\": \"python\"}}.\n\
             Do NOT put actual code in the input — just describe the task.\n\
             For the \"research\" skill, use {{\"query\": \"what to research\"}}.\n\n\
             Return ONLY valid JSON array, no markdown fences, no explanation.",
            goal = task.goal,
        );

        let plan_response = self.llm.plan(vec![
            ChatMessage { role: "user".into(), content: plan_prompt },
        ]).await?;

        // Parse plan — strip markdown fences if present
        let plan_str = plan_response.trim();
        let plan_str = if plan_str.starts_with("```") {
            plan_str.lines()
                .skip(1)
                .take_while(|l| !l.starts_with("```"))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            plan_str.to_string()
        };

        let plan: Vec<Value> = serde_json::from_str(&plan_str)
            .map_err(|e| AppError::Llm(format!("Failed to parse plan: {e}\nResponse: {plan_response}")))?;

        db::set_task_plan(&self.pool, task_id, json!(&plan)).await?;
        self.emit(task_id, "plan_ready", json!({"steps": plan.len()})).await;

        // Create steps in DB
        for (i, step) in plan.iter().enumerate() {
            let skill = step["skill"].as_str().unwrap_or("unknown");
            let description = step["description"].as_str().unwrap_or("");
            db::create_task_step(&self.pool, task_id, skill, description, i as i32).await?;
        }

        // Phase 2: Execute
        db::update_task_status(&self.pool, task_id, TaskStatus::Running).await?;
        let container_id = self.sandbox.acquire(&task_id.to_string()).await?;
        self.emit(task_id, "sandbox_ready", json!({"container_id": &container_id[..12.min(container_id.len())]})).await;

        let steps = db::get_task_steps(&self.pool, task_id).await?;
        let mut last_result = json!({});

        for (i, step) in steps.iter().enumerate() {
            self.emit(task_id, "step_started", json!({
                "step": i + 1,
                "total": steps.len(),
                "skill": step.skill,
                "description": step.description,
            })).await;

            db::update_step_status(&self.pool, step.id, TaskStatus::Running).await?;

            let skill = self.skills.get(&step.skill)
                .ok_or_else(|| AppError::Internal(format!("Unknown skill: {}", step.skill)))?;

            let ctx = SkillContext {
                llm: self.llm.clone(),
                sandbox: self.sandbox.clone(),
                container_id: container_id.clone(),
                task_id: task_id.to_string(),
            };

            let mut input = plan[i]["input"].clone();
            if let Some(obj) = input.as_object_mut() {
                obj.insert("_previous_result".into(), last_result.clone());
            }

            match skill.execute(&ctx, input).await {
                Ok(output) => {
                    last_result = output.result.clone();
                    db::update_step_status(&self.pool, step.id, TaskStatus::Completed).await?;
                    self.emit(task_id, "step_completed", json!({
                        "step": i + 1,
                        "result_preview": output.result.to_string().chars().take(500).collect::<String>(),
                    })).await;
                    db::add_task_event(&self.pool, task_id, "step_completed",
                        json!({"step": i + 1, "skill": step.skill})).await?;
                }
                Err(e) => {
                    db::update_step_status(&self.pool, step.id, TaskStatus::Failed).await?;
                    db::set_task_error(&self.pool, task_id, &e.to_string()).await?;
                    self.emit(task_id, "step_failed", json!({
                        "step": i + 1,
                        "error": e.to_string(),
                    })).await;
                    // Release sandbox back to pool (don't destroy)
                    let _ = self.sandbox.release(&container_id).await;
                    self.emit(task_id, "task_failed", json!({"error": e.to_string()})).await;
                    return Err(e);
                }
            }
        }

        // Phase 3: Complete
        db::set_task_result(&self.pool, task_id, last_result.clone()).await?;
        self.emit(task_id, "task_completed", json!({"result": last_result})).await;

        // Release sandbox back to pool
        let _ = self.sandbox.release(&container_id).await;
        Ok(())
    }

    /// Publish event via Redis Pub/Sub
    async fn emit(&self, task_id: Uuid, event_type: &str, data: Value) {
        let event = TaskEvent {
            task_id: task_id.to_string(),
            event_type: event_type.to_string(),
            data,
        };
        if let Err(e) = self.redis.publish_event(&event).await {
            tracing::error!("Failed to publish event: {e}");
        }
    }
}

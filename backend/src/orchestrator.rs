use serde_json::{json, Value};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::agent_runtime::{AgentRegistry, AgentRuntime};
use crate::db;
use crate::error::AppError;
use crate::llm::{ChatMessage, LlmClient};
use crate::models::{TaskStatus, UserMemory};
use crate::redis_client::{RedisClient, TaskEvent};
use crate::sandbox::SandboxManager;
use crate::soul;
use crate::tools::SandboxHandle;

#[derive(Clone)]
pub struct Orchestrator {
    pool: PgPool,
    llm: LlmClient,
    sandbox: SandboxManager,
    agents: Arc<AgentRegistry>,
    runtime: Arc<AgentRuntime>,
    redis: RedisClient,
    cancellation_tokens: Arc<RwLock<HashMap<Uuid, CancellationToken>>>,
}

impl Orchestrator {
    pub fn new(
        pool: PgPool,
        llm: LlmClient,
        sandbox: SandboxManager,
        agents: Arc<AgentRegistry>,
        redis: RedisClient,
    ) -> Self {
        let runtime = Arc::new(AgentRuntime::new(llm.clone(), agents.clone()));
        Self {
            pool, llm, sandbox, agents, runtime, redis,
            cancellation_tokens: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Cancel a running task
    pub async fn cancel_task(&self, task_id: Uuid) -> Result<(), AppError> {
        let task = db::get_task(&self.pool, task_id).await?;
        if !matches!(task.status, TaskStatus::Pending | TaskStatus::Planning | TaskStatus::Running) {
            return Err(AppError::BadRequest(format!(
                "Task is not cancellable (status: {:?})", task.status
            )));
        }

        // Cancel the token if one exists
        let tokens = self.cancellation_tokens.read().await;
        if let Some(token) = tokens.get(&task_id) {
            token.cancel();
        }
        drop(tokens);

        // Update DB
        db::set_task_error(&self.pool, task_id, "Cancelled by user").await?;
        db::update_task_status(&self.pool, task_id, TaskStatus::Failed).await?;

        // Emit cancellation event
        self.emit(task_id, "task_cancelled", json!({"reason": "Cancelled by user"})).await;

        // Clean up token
        self.cancellation_tokens.write().await.remove(&task_id);

        Ok(())
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

    /// Execute a single task end-to-end with self-reflection and adaptive re-planning
    async fn execute_task(&self, task_id: Uuid) -> Result<(), AppError> {
        let token = CancellationToken::new();
        self.cancellation_tokens.write().await.insert(task_id, token.clone());

        let result = self.execute_task_inner(task_id, &token).await;

        // Clean up token
        self.cancellation_tokens.write().await.remove(&task_id);

        result
    }

    async fn execute_task_inner(&self, task_id: Uuid, token: &CancellationToken) -> Result<(), AppError> {
        let start = Instant::now();
        let task = db::get_task(&self.pool, task_id).await?;
        self.emit(task_id, "task_started", json!({"goal": task.goal})).await;

        // Load cross-task user memory
        let user_memories = db::get_all_user_memories(&self.pool, &task.user_id, 50).await
            .unwrap_or_default();
        let user_context = if user_memories.is_empty() {
            None
        } else {
            Some(Self::format_user_memory(&user_memories))
        };

        // Phase 1: Plan
        db::update_task_status(&self.pool, task_id, TaskStatus::Planning).await?;
        self.emit(task_id, "planning", json!({})).await;

        let plan = self.create_plan(&task.goal, user_context.as_deref()).await?;

        db::set_task_plan(&self.pool, task_id, json!(&plan)).await?;
        self.emit(task_id, "plan_ready", json!({"steps": plan.len()})).await;

        // Create steps in DB
        for (i, step) in plan.iter().enumerate() {
            let skill = step["skill"].as_str().unwrap_or("unknown");
            let description = step["description"].as_str().unwrap_or("");
            db::create_task_step(&self.pool, task_id, skill, description, i as i32).await?;
        }

        // Phase 2: Execute with self-reflection
        db::update_task_status(&self.pool, task_id, TaskStatus::Running).await?;
        let container_id = self.sandbox.acquire(&task_id.to_string()).await?;
        self.emit(task_id, "sandbox_ready", json!({
            "container_id": &container_id[..12.min(container_id.len())]
        })).await;

        // Store initial context in memory
        db::set_memory(&self.pool, task_id, "goal", json!(task.goal)).await?;
        db::set_memory(&self.pool, task_id, "plan", json!(&plan)).await?;

        let steps = db::get_task_steps(&self.pool, task_id).await?;
        let mut step_results: Vec<Value> = Vec::new();
        let mut all_artifacts: Vec<String> = Vec::new();
        let current_plan = plan;

        // Create a single SandboxHandle shared across all steps
        let sandbox_handle = SandboxHandle::new(self.sandbox.clone(), container_id.clone());
        let task_id_str = task_id.to_string();

        for (i, step) in steps.iter().enumerate() {
            // Check for cancellation before each step
            if token.is_cancelled() {
                let _ = self.sandbox.release(&container_id).await;
                let duration = start.elapsed().as_millis() as i64;
                let _ = db::set_task_duration(&self.pool, task_id, duration).await;
                return Ok(());
            }

            self.emit(task_id, "step_started", json!({
                "step": i + 1,
                "total": steps.len(),
                "skill": step.skill,
                "description": step.description,
            })).await;

            db::update_step_status(&self.pool, step.id, TaskStatus::Running).await?;

            // Look up agent by name (step.skill now contains agent names)
            let agent = match self.agents.get(&step.skill) {
                Some(a) => a,
                None => {
                    tracing::warn!("Unknown agent '{}', attempting code fallback", step.skill);
                    self.emit(task_id, "step_warning", json!({
                        "step": i + 1,
                        "warning": format!("Unknown agent '{}', using code fallback", step.skill),
                    })).await;
                    match self.agents.get("code") {
                        Some(a) => a,
                        None => {
                            db::update_step_status(&self.pool, step.id, TaskStatus::Failed).await?;
                            continue;
                        }
                    }
                }
            };

            // Retry loop: up to 3 attempts (1 initial + 2 retries)
            const MAX_ATTEMPTS: usize = 3;
            let mut last_error: Option<AppError> = None;

            for attempt in 0..MAX_ATTEMPTS {
                // Build goal: on retry, prepend reflection about previous failure
                let goal = if attempt == 0 {
                    step.description.clone()
                } else {
                    let err_msg = last_error.as_ref().map(|e| e.to_string()).unwrap_or_default();
                    format!(
                        "IMPORTANT: Previous attempt failed with: {}. Try a completely different approach.\n\nOriginal task: {}",
                        err_msg, step.description
                    )
                };

                let result = self.runtime.run(
                    &agent,
                    &goal,
                    &sandbox_handle,
                    &task_id_str,
                    &self.redis,
                    0,
                    &self.pool,
                    Some(step.id),
                ).await;

                match result {
                    Ok(agent_result) => {
                        let step_output = json!({"output": agent_result.output});
                        step_results.push(step_output.clone());
                        all_artifacts.extend(agent_result.artifacts.clone());

                        // Store artifacts in DB
                        for artifact_path in &agent_result.artifacts {
                            let name = artifact_path.rsplit('/').next().unwrap_or(artifact_path);
                            let artifact_type = Self::infer_artifact_type(name);
                            let mime = Self::infer_mime_type(name);
                            let _ = db::create_artifact(
                                &self.pool, task_id, Some(step.id),
                                name, &artifact_type, Some(&mime),
                                Some(artifact_path), None, None, None,
                            ).await;
                        }

                        db::update_step_status(&self.pool, step.id, TaskStatus::Completed).await?;
                        self.emit(task_id, "step_completed", json!({
                            "step": i + 1,
                            "skill": step.skill,
                            "artifacts": agent_result.artifacts,
                            "result_preview": agent_result.output.chars().take(500).collect::<String>(),
                            "turns_used": agent_result.turns_used,
                        })).await;
                        db::add_task_event(&self.pool, task_id, "step_completed",
                            json!({"step": i + 1, "skill": step.skill})).await?;

                        // Update memory with latest result
                        db::set_memory(&self.pool, task_id, &format!("step_{}_result", i + 1),
                            step_output).await?;

                        last_error = None;
                        break;
                    }
                    Err(e) => {
                        let err_str = e.to_string();
                        db::set_step_error(&self.pool, step.id, &err_str).await?;

                        if attempt < MAX_ATTEMPTS - 1 {
                            // Not the last attempt — record retry info and try again
                            let retry_count = db::increment_step_retry(&self.pool, step.id).await?;
                            let reflection = format!(
                                "Attempt {} failed: {}. Retrying with different approach.",
                                attempt + 1, err_str
                            );
                            db::set_step_reflection(&self.pool, step.id, &reflection).await?;

                            self.emit(task_id, "step_retrying", json!({
                                "step": i + 1,
                                "attempt": attempt + 1,
                                "max_attempts": MAX_ATTEMPTS,
                                "retry_count": retry_count,
                                "error": err_str,
                            })).await;

                            last_error = Some(e);
                            // continue to next attempt
                        } else {
                            // Final attempt exhausted
                            last_error = Some(e);
                        }
                    }
                }
            }

            // If all retries exhausted and step still failed
            if let Some(e) = last_error {
                db::update_step_status(&self.pool, step.id, TaskStatus::Failed).await?;
                self.emit(task_id, "step_failed", json!({
                    "step": i + 1,
                    "error": e.to_string(),
                    "retries_exhausted": true,
                })).await;

                // Try adaptive re-planning if there are remaining steps
                if i < steps.len() - 1 {
                    self.emit(task_id, "replanning", json!({
                        "reason": format!("Step {} ({}) failed after {} attempts: {}", i + 1, step.skill, MAX_ATTEMPTS, e),
                    })).await;

                    match self.replan(
                        &task.goal, &current_plan, &step_results, i, &e.to_string(),
                    ).await {
                        Ok(new_plan) => {
                            self.emit(task_id, "replan_ready", json!({
                                "new_steps": new_plan.len(),
                            })).await;

                            // Create new step records in DB and execute them
                            let base_order = steps.len() as i32;
                            for (j, new_step_plan) in new_plan.iter().enumerate() {
                                let skill = new_step_plan["skill"].as_str().unwrap_or("code");
                                let description = new_step_plan["description"].as_str().unwrap_or("");
                                let new_step = db::create_task_step(
                                    &self.pool, task_id, skill, description, base_order + j as i32,
                                ).await?;

                                // Check cancellation before each replanned step
                                if token.is_cancelled() {
                                    let _ = self.sandbox.release(&container_id).await;
                                    let duration = start.elapsed().as_millis() as i64;
                                    let _ = db::set_task_duration(&self.pool, task_id, duration).await;
                                    return Ok(());
                                }

                                self.emit(task_id, "step_started", json!({
                                    "step": format!("replan-{}", j + 1),
                                    "total": new_plan.len(),
                                    "skill": skill,
                                    "description": description,
                                })).await;

                                db::update_step_status(&self.pool, new_step.id, TaskStatus::Running).await?;

                                let replan_agent = match self.agents.get(skill) {
                                    Some(a) => a,
                                    None => {
                                        match self.agents.get("code") {
                                            Some(a) => a,
                                            None => {
                                                db::update_step_status(&self.pool, new_step.id, TaskStatus::Failed).await?;
                                                continue;
                                            }
                                        }
                                    }
                                };

                                let replan_result = self.runtime.run(
                                    &replan_agent,
                                    description,
                                    &sandbox_handle,
                                    &task_id_str,
                                    &self.redis,
                                    0,
                                    &self.pool,
                                    Some(new_step.id),
                                ).await;

                                match replan_result {
                                    Ok(agent_result) => {
                                        let step_output = json!({"output": agent_result.output});
                                        step_results.push(step_output.clone());
                                        all_artifacts.extend(agent_result.artifacts.clone());

                                        for artifact_path in &agent_result.artifacts {
                                            let name = artifact_path.rsplit('/').next().unwrap_or(artifact_path);
                                            let artifact_type = Self::infer_artifact_type(name);
                                            let mime = Self::infer_mime_type(name);
                                            let _ = db::create_artifact(
                                                &self.pool, task_id, Some(new_step.id),
                                                name, &artifact_type, Some(&mime),
                                                Some(artifact_path), None, None, None,
                                            ).await;
                                        }

                                        db::update_step_status(&self.pool, new_step.id, TaskStatus::Completed).await?;
                                        self.emit(task_id, "step_completed", json!({
                                            "step": format!("replan-{}", j + 1),
                                            "skill": skill,
                                            "artifacts": agent_result.artifacts,
                                            "result_preview": agent_result.output.chars().take(500).collect::<String>(),
                                            "turns_used": agent_result.turns_used,
                                        })).await;
                                    }
                                    Err(replan_step_err) => {
                                        db::update_step_status(&self.pool, new_step.id, TaskStatus::Failed).await?;
                                        db::set_task_error(&self.pool, task_id, &replan_step_err.to_string()).await?;
                                        let _ = self.sandbox.release(&container_id).await;
                                        let duration = start.elapsed().as_millis() as i64;
                                        let _ = db::set_task_duration(&self.pool, task_id, duration).await;
                                        self.emit(task_id, "task_failed", json!({"error": replan_step_err.to_string()})).await;
                                        return Err(replan_step_err);
                                    }
                                }
                            }

                            // Successfully executed all replanned steps — break out of original loop
                            break;
                        }
                        Err(replan_err) => {
                            tracing::error!("Re-planning failed: {replan_err}");
                            // Fall through to task failure
                        }
                    }
                }

                // No remaining steps or re-planning failed — fail the task
                db::set_task_error(&self.pool, task_id, &e.to_string()).await?;
                let _ = self.sandbox.release(&container_id).await;
                let duration = start.elapsed().as_millis() as i64;
                let _ = db::set_task_duration(&self.pool, task_id, duration).await;
                self.emit(task_id, "task_failed", json!({"error": e.to_string()})).await;
                return Err(e);
            }
        }

        // Phase 3: Synthesize final result
        let final_result = self.synthesize_result(
            &task.goal, &step_results, &all_artifacts,
        ).await?;

        db::set_task_result(&self.pool, task_id, final_result.clone()).await?;

        // Phase 4: Extract learnings into cross-task memory
        if let Err(e) = self.extract_learnings(&task.user_id, &task.goal, &final_result).await {
            tracing::warn!("Failed to extract learnings: {e}");
        } else {
            self.emit(task_id, "learnings_stored", json!({"user_id": task.user_id})).await;
        }

        let duration = start.elapsed().as_millis() as i64;
        let _ = db::set_task_duration(&self.pool, task_id, duration).await;

        self.emit(task_id, "task_completed", json!({
            "result": final_result,
            "artifacts": all_artifacts,
            "duration_ms": duration,
        })).await;

        let _ = self.sandbox.release(&container_id).await;
        Ok(())
    }

    /// Create the initial execution plan
    async fn create_plan(&self, goal: &str, user_context: Option<&str>) -> Result<Vec<Value>, AppError> {
        let agent_list = self.agents.list();
        let agent_descriptions = agent_list.iter()
            .map(|(name, desc)| format!("- {name}: {desc}"))
            .collect::<Vec<_>>()
            .join("\n");

        let role_instructions = "You are a world-class autonomous AI agent planner. Your job is to decompose \
             complex goals into a precise sequence of executable steps, each delegated to a specialist agent.";

        let system = soul::system_prompt(role_instructions, user_context);

        let plan_prompt = format!(
            "Goal: {goal}\n\n\
             Available agents:\n{agent_descriptions}\n\n\
             PLANNING GUIDELINES:\n\
             1. Break the goal into the MINIMUM number of steps needed\n\
             2. Assign the most appropriate agent for each step\n\
             3. Each agent is autonomous: give it a clear description of what to accomplish and it will figure out how\n\
             4. For web browsing/scraping tasks, use 'browser' agent\n\
             5. For research/information gathering, use 'research' agent\n\
             6. For coding/programming tasks, use 'code' agent\n\
             7. For data processing/analysis, use 'data_analysis' agent\n\
             8. For API integrations, use 'api' agent\n\
             9. For deployment, use 'deploy' agent as the final step\n\
             10. Steps with no dependencies can run IN PARALLEL — only add dependencies where one step genuinely needs the output of another\n\n\
             Return a JSON array of steps. Each step has:\n\
             - \"skill\": agent name (MUST be one from the list above)\n\
             - \"description\": a clear, detailed description of what this step should accomplish\n\
             - \"depends_on\": array of 0-based step indices this step depends on (use [] if independent)\n\n\
             The description is the agent's goal — be specific about what output you expect.\n\
             Do NOT include an \"input\" field; agents determine their own approach.\n\n\
             Return ONLY valid JSON array, no markdown fences, no explanation."
        );

        let plan_response = self.llm.plan(vec![
            ChatMessage { role: "system".into(), content: system },
            ChatMessage { role: "user".into(), content: plan_prompt },
        ]).await?;

        Self::parse_json_array(&plan_response)
    }

    /// Adaptive re-planning when a step fails
    async fn replan(
        &self,
        goal: &str,
        current_plan: &[Value],
        completed_results: &[Value],
        failed_step: usize,
        error: &str,
    ) -> Result<Vec<Value>, AppError> {
        let completed_summary: Vec<_> = current_plan.iter().take(failed_step)
            .enumerate()
            .map(|(i, step)| {
                let result_preview = completed_results.get(i)
                    .map(|r| r.to_string().chars().take(200).collect::<String>())
                    .unwrap_or_default();
                format!("Step {}: {} [{}] — Completed. Result: {}",
                    i + 1,
                    step["description"].as_str().unwrap_or(""),
                    step["skill"].as_str().unwrap_or(""),
                    result_preview,
                )
            })
            .collect();

        let failed_step_desc = current_plan.get(failed_step)
            .map(|s| format!("{} [{}]",
                s["description"].as_str().unwrap_or(""),
                s["skill"].as_str().unwrap_or("")
            ))
            .unwrap_or_default();

        let remaining: Vec<_> = current_plan.iter().skip(failed_step + 1)
            .map(|s| format!("{} [{}]",
                s["description"].as_str().unwrap_or(""),
                s["skill"].as_str().unwrap_or("")
            ))
            .collect();

        let agent_list = self.agents.list();
        let agent_descriptions = agent_list.iter()
            .map(|(name, desc)| format!("- {name}: {desc}"))
            .collect::<Vec<_>>()
            .join("\n");

        let system = soul::system_prompt(
            "You are an adaptive re-planner. A step failed during task execution. \
             Create a NEW plan for the REMAINING work, taking into account what's already done.\n\
             Return ONLY a JSON array of new steps. Each step has \"skill\" (agent name) and \"description\".\n\
             Try a different approach for the failed step.",
            None,
        );
        let messages = vec![
            ChatMessage {
                role: "system".into(),
                content: system,
            },
            ChatMessage {
                role: "user".into(),
                content: format!(
                    "Goal: {goal}\n\n\
                     Completed steps:\n{completed}\n\n\
                     FAILED step: {failed_step_desc}\nError: {error}\n\n\
                     Remaining planned steps:\n{remaining}\n\n\
                     Available agents:\n{agent_descriptions}\n\n\
                     Create a new plan for the REMAINING work (different approach for the failed step).\n\
                     Return ONLY valid JSON array.",
                    completed = completed_summary.join("\n"),
                    remaining = remaining.join("\n"),
                ),
            },
        ];

        let response = self.llm.plan(messages).await?;
        Self::parse_json_array(&response)
    }

    /// Synthesize a final human-readable result from all step outputs
    async fn synthesize_result(
        &self,
        goal: &str,
        step_results: &[Value],
        artifacts: &[String],
    ) -> Result<Value, AppError> {
        let results_summary: Vec<String> = step_results.iter().enumerate()
            .map(|(i, r)| format!("Step {}: {}", i + 1, r.to_string().chars().take(500).collect::<String>()))
            .collect();

        let system = soul::system_prompt(
            "You are a result synthesizer. Create a clear, structured summary of \
             what was accomplished. Return a JSON object with:\n\
             {\"summary\": \"brief overview\", \"key_outputs\": [\"...\"], \"artifacts\": [\"...\"], \"next_steps\": [\"...\"]}",
            None,
        );
        let messages = vec![
            ChatMessage {
                role: "system".into(),
                content: system,
            },
            ChatMessage {
                role: "user".into(),
                content: format!(
                    "Goal: {goal}\n\nStep results:\n{}\n\nArtifacts produced: {:?}",
                    results_summary.join("\n\n"),
                    artifacts,
                ),
            },
        ];

        let response = self.llm.fast(messages).await?;
        // Strip markdown code fences if present
        let clean = response.trim();
        let clean = if clean.starts_with("```") {
            clean.lines()
                .skip(1)
                .take_while(|l| !l.starts_with("```"))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            clean.to_string()
        };
        let parsed: Value = serde_json::from_str(&clean)
            .unwrap_or_else(|_| json!({
                "summary": response,
                "key_outputs": step_results.to_vec(),
                "artifacts": artifacts,
            }));
        Ok(parsed)
    }

    /// Format user memories into a text block for inclusion in prompts.
    fn format_user_memory(memories: &[UserMemory]) -> String {
        let mut out = String::new();
        let categories = ["preference", "fact", "learning"];
        for cat in &categories {
            let items: Vec<_> = memories.iter()
                .filter(|m| m.category == *cat)
                .take(20)
                .collect();
            if items.is_empty() {
                continue;
            }
            out.push_str(&format!("### {}\n", cat.to_uppercase()));
            for m in items {
                let value_str = match m.value.as_str() {
                    Some(s) => s.to_string(),
                    None => m.value.to_string(),
                };
                out.push_str(&format!("- **{}**: {}\n", m.key, value_str));
            }
            out.push('\n');
        }
        out
    }

    /// Extract learnings from a completed task and store in user memory.
    async fn extract_learnings(
        &self,
        user_id: &str,
        goal: &str,
        result: &Value,
    ) -> Result<(), AppError> {
        let system = soul::system_prompt(
            "You extract reusable learnings from completed tasks. Return a JSON array of objects, \
             each with {\"category\": \"preference|fact|learning\", \"key\": \"short_key\", \"value\": \"description\"}.\n\
             Categories:\n\
             - preference: user's style/format preferences observed from the task\n\
             - fact: factual info about the user's domain, stack, or context\n\
             - learning: what approach worked well and could help future tasks\n\n\
             Return 0-5 items. Only include genuinely reusable insights, not task-specific details.\n\
             Return ONLY valid JSON array.",
            None,
        );

        let result_preview = result.to_string();
        let result_preview = if result_preview.len() > 2000 {
            &result_preview[..2000]
        } else {
            &result_preview
        };

        let messages = vec![
            ChatMessage { role: "system".into(), content: system },
            ChatMessage {
                role: "user".into(),
                content: format!(
                    "Completed task goal: {goal}\n\nResult:\n{result_preview}\n\n\
                     What reusable learnings can be extracted?"
                ),
            },
        ];

        let response = self.llm.fast(messages).await?;
        let learnings: Vec<Value> = Self::parse_json_array(&response).unwrap_or_default();

        for item in &learnings {
            let category = item["category"].as_str().unwrap_or("learning");
            let Some(key) = item["key"].as_str() else { continue };
            let value = item.get("value").cloned().unwrap_or(json!(""));
            db::set_user_memory(&self.pool, user_id, category, key, value).await?;
        }

        if !learnings.is_empty() {
            db::prune_user_memories(&self.pool, user_id, 200).await?;
            tracing::info!(user_id, count = learnings.len(), "Stored user learnings");
        }

        Ok(())
    }

    /// Parse a JSON array from LLM response, handling markdown fences
    fn parse_json_array(response: &str) -> Result<Vec<Value>, AppError> {
        let plan_str = response.trim();
        let plan_str = if plan_str.starts_with("```") {
            plan_str.lines()
                .skip(1)
                .take_while(|l| !l.starts_with("```"))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            plan_str.to_string()
        };

        serde_json::from_str(&plan_str)
            .map_err(|e| AppError::Llm(format!("Failed to parse JSON: {e}\nResponse: {response}")))
    }

    /// Infer artifact type from filename
    fn infer_artifact_type(name: &str) -> String {
        let lower = name.to_lowercase();
        if lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg")
            || lower.ends_with(".gif") || lower.ends_with(".svg") {
            "screenshot".to_string()
        } else if lower.ends_with(".md") || lower.ends_with(".txt") {
            "report".to_string()
        } else if lower.ends_with(".py") || lower.ends_with(".js") || lower.ends_with(".ts")
            || lower.ends_with(".rs") || lower.ends_with(".go") {
            "code".to_string()
        } else if lower.ends_with(".csv") || lower.ends_with(".json") {
            "data".to_string()
        } else if lower.ends_with(".tar.gz") || lower.ends_with(".zip") {
            "deployment".to_string()
        } else {
            "file".to_string()
        }
    }

    /// Infer MIME type from filename
    fn infer_mime_type(name: &str) -> String {
        let lower = name.to_lowercase();
        if lower.ends_with(".png") { "image/png".into() }
        else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") { "image/jpeg".into() }
        else if lower.ends_with(".gif") { "image/gif".into() }
        else if lower.ends_with(".svg") { "image/svg+xml".into() }
        else if lower.ends_with(".html") { "text/html".into() }
        else if lower.ends_with(".css") { "text/css".into() }
        else if lower.ends_with(".js") { "application/javascript".into() }
        else if lower.ends_with(".json") { "application/json".into() }
        else if lower.ends_with(".csv") { "text/csv".into() }
        else if lower.ends_with(".md") { "text/markdown".into() }
        else if lower.ends_with(".pdf") { "application/pdf".into() }
        else if lower.ends_with(".tar.gz") { "application/gzip".into() }
        else if lower.ends_with(".zip") { "application/zip".into() }
        else { "application/octet-stream".into() }
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

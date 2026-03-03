use serde_json::{json, Value};
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::agent_runtime::{AgentRegistry, AgentRuntime};
use crate::db;
use crate::error::AppError;
use crate::llm::{ChatMessage, LlmClient};
use crate::models::{InputArtifact, TaskStatus, TaskStep, UserMemory};
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

        // Task-level timeout: 30 minutes max
        const TASK_TIMEOUT: Duration = Duration::from_secs(30 * 60);
        let result = match tokio::time::timeout(TASK_TIMEOUT, self.execute_task_inner(task_id, &token)).await {
            Ok(r) => r,
            Err(_) => {
                tracing::error!("Task {task_id} timed out after 30 minutes");
                db::set_task_error(&self.pool, task_id, "Task timed out after 30 minutes").await?;
                db::update_task_status(&self.pool, task_id, TaskStatus::Failed).await?;
                self.emit(task_id, "task_failed", json!({"error": "Task timed out after 30 minutes"})).await;
                Err(AppError::Internal("Task timed out".into()))
            }
        };

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

        // Load input artifacts for this task
        let input_artifacts = db::get_input_artifacts(&self.pool, task_id).await
            .unwrap_or_default();

        // Phase 1: Plan
        db::update_task_status(&self.pool, task_id, TaskStatus::Planning).await?;
        self.emit(task_id, "planning", json!({})).await;

        let plan = self.create_plan(&task.goal, user_context.as_deref(), &input_artifacts).await?;

        db::set_task_plan(&self.pool, task_id, json!(&plan)).await?;
        self.emit(task_id, "plan_ready", json!({"steps": plan.len()})).await;

        // Create steps in DB
        for (i, step) in plan.iter().enumerate() {
            let skill = step["skill"].as_str().unwrap_or("unknown");
            let description = step["description"].as_str().unwrap_or("");
            db::create_task_step(&self.pool, task_id, skill, description, i as i32).await?;
        }

        // Phase 2: Execute steps with dependency-aware parallel dispatch
        db::update_task_status(&self.pool, task_id, TaskStatus::Running).await?;

        // Store initial context in memory
        db::set_memory(&self.pool, task_id, "goal", json!(task.goal)).await?;
        db::set_memory(&self.pool, task_id, "plan", json!(&plan)).await?;

        let steps = db::get_task_steps(&self.pool, task_id).await?;
        let current_plan = plan;

        // Parse dependency graph from plan
        let deps: Vec<Vec<usize>> = current_plan.iter().map(|s| {
            s["depends_on"].as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_u64().map(|n| n as usize)).collect())
                .unwrap_or_default()
        }).collect();

        let total_steps = steps.len();
        let mut completed: HashSet<usize> = HashSet::new();
        let mut failed: HashSet<usize> = HashSet::new();
        let mut step_results: Vec<Option<Value>> = vec![None; total_steps];
        let mut all_artifacts: Vec<String> = Vec::new();
        let mut total_usage = crate::llm::TokenUsage::default();

        while completed.len() + failed.len() < total_steps {
            // Check for cancellation
            if token.is_cancelled() {
                let duration = start.elapsed().as_millis() as i64;
                let _ = db::set_task_duration(&self.pool, task_id, duration).await;
                return Ok(());
            }

            // Find ready steps: not completed, not failed, no deps in failed, all deps completed
            let ready: Vec<usize> = (0..total_steps)
                .filter(|i| !completed.contains(i) && !failed.contains(i))
                .filter(|i| !deps[*i].iter().any(|d| failed.contains(d)))
                .filter(|i| deps[*i].iter().all(|d| completed.contains(d)))
                .collect();

            // If no steps are ready but some are still pending, they are blocked by failed deps
            if ready.is_empty() {
                for i in 0..total_steps {
                    if !completed.contains(&i) && !failed.contains(&i) {
                        failed.insert(i);
                        let _ = db::update_step_status(&self.pool, steps[i].id, TaskStatus::Failed).await;
                        self.emit(task_id, "step_failed", json!({
                            "step": i + 1,
                            "error": "Blocked by failed dependency",
                        })).await;
                    }
                }
                break;
            }

            // Spawn ready steps into a JoinSet for parallel execution
            let mut join_set = tokio::task::JoinSet::new();

            for &idx in &ready {
                let step = steps[idx].clone();
                let plan_step = current_plan[idx].clone();
                let orch = self.clone();
                let token_clone = token.clone();
                let artifacts_clone = input_artifacts.clone();

                join_set.spawn(async move {
                    // Each parallel step acquires its own sandbox container
                    let container_id = orch.sandbox.acquire(&task_id.to_string()).await?;
                    let sandbox_handle = SandboxHandle::new(orch.sandbox.clone(), container_id.clone());

                    orch.emit(task_id, "sandbox_ready", json!({
                        "container_id": &container_id[..12.min(container_id.len())],
                        "step": idx + 1,
                    })).await;

                    // Write input artifacts into the sandbox before the agent runs
                    if let Err(e) = Self::write_input_artifacts(&sandbox_handle, &artifacts_clone).await {
                        tracing::warn!("Failed to write input artifacts for step {}: {e}", idx + 1);
                    }

                    let result = orch.execute_single_step(
                        task_id, &step, idx, total_steps, &plan_step,
                        &sandbox_handle, &token_clone,
                    ).await;

                    // Release sandbox container back to pool
                    let _ = orch.sandbox.release(&container_id).await;

                    Ok::<(usize, Result<(Value, Vec<String>, crate::llm::TokenUsage), AppError>), AppError>((idx, result))
                });
            }

            // Collect results from all spawned steps
            while let Some(join_result) = join_set.join_next().await {
                match join_result {
                    Ok(Ok((idx, Ok((result, artifacts, usage))))) => {
                        step_results[idx] = Some(result);
                        all_artifacts.extend(artifacts);
                        total_usage.prompt_tokens += usage.prompt_tokens;
                        total_usage.completion_tokens += usage.completion_tokens;
                        total_usage.total_tokens += usage.total_tokens;
                        completed.insert(idx);
                    }
                    Ok(Ok((idx, Err(e)))) => {
                        tracing::error!("Step {} failed: {e}", idx + 1);
                        failed.insert(idx);
                    }
                    Ok(Err(e)) => {
                        // Sandbox acquisition error — mark all ready steps in this batch as failed
                        tracing::error!("Sandbox acquisition error: {e}");
                        for &idx in &ready {
                            if !completed.contains(&idx) && !failed.contains(&idx) {
                                failed.insert(idx);
                                let _ = db::update_step_status(&self.pool, steps[idx].id, TaskStatus::Failed).await;
                                self.emit(task_id, "step_failed", json!({
                                    "step": idx + 1,
                                    "error": format!("Sandbox acquisition failed: {e}"),
                                })).await;
                            }
                        }
                    }
                    Err(e) => {
                        // JoinError (task panicked) — mark all ready steps in this batch as failed
                        tracing::error!("Step task panicked: {e}");
                        for &idx in &ready {
                            if !completed.contains(&idx) && !failed.contains(&idx) {
                                failed.insert(idx);
                                let _ = db::update_step_status(&self.pool, steps[idx].id, TaskStatus::Failed).await;
                                self.emit(task_id, "step_failed", json!({
                                    "step": idx + 1,
                                    "error": format!("Task panicked: {e}"),
                                })).await;
                            }
                        }
                    }
                }
            }
        }

        // If every step failed, fail the whole task
        if completed.is_empty() {
            let err_msg = "All steps failed";
            db::set_task_error(&self.pool, task_id, err_msg).await?;
            let duration = start.elapsed().as_millis() as i64;
            let _ = db::set_task_duration(&self.pool, task_id, duration).await;
            self.emit(task_id, "task_failed", json!({"error": err_msg})).await;
            return Err(AppError::Internal(err_msg.to_string()));
        }

        // Phase 3: Synthesize final result from completed steps
        let final_step_results: Vec<Value> = step_results.into_iter().flatten().collect();

        let final_result = self.synthesize_result(
            &task.goal, &final_step_results, &all_artifacts,
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
        let _ = db::set_task_token_usage(&self.pool, task_id, json!({
            "prompt_tokens": total_usage.prompt_tokens,
            "completion_tokens": total_usage.completion_tokens,
            "total_tokens": total_usage.total_tokens,
        })).await;

        self.emit(task_id, "task_completed", json!({
            "result": final_result,
            "artifacts": all_artifacts,
            "duration_ms": duration,
        })).await;

        Ok(())
    }

    /// Execute a single step with retry logic.
    ///
    /// Encapsulates agent lookup, retry loop with reflection, artifact storage,
    /// event emission, and DB updates for one step. Used by the parallel dispatch
    /// loop — each spawned task calls this with its own sandbox handle.
    async fn execute_single_step(
        &self,
        task_id: Uuid,
        step: &TaskStep,
        step_index: usize,
        total_steps: usize,
        _plan_step: &Value,
        sandbox_handle: &SandboxHandle,
        token: &CancellationToken,
    ) -> Result<(Value, Vec<String>, crate::llm::TokenUsage), AppError> {
        // Check cancellation
        if token.is_cancelled() {
            return Err(AppError::Internal("Task cancelled".into()));
        }

        self.emit(task_id, "step_started", json!({
            "step": step_index + 1,
            "total": total_steps,
            "skill": step.skill,
            "description": step.description,
        })).await;

        db::update_step_status(&self.pool, step.id, TaskStatus::Running).await?;

        // Look up agent by name (step.skill contains agent names)
        let agent = match self.agents.get(&step.skill) {
            Some(a) => a,
            None => {
                tracing::warn!("Unknown agent '{}', attempting code fallback", step.skill);
                self.emit(task_id, "step_warning", json!({
                    "step": step_index + 1,
                    "warning": format!("Unknown agent '{}', using code fallback", step.skill),
                })).await;
                match self.agents.get("code") {
                    Some(a) => a,
                    None => {
                        db::update_step_status(&self.pool, step.id, TaskStatus::Failed).await?;
                        return Err(AppError::Internal(format!(
                            "No agent found for '{}' and no code fallback available", step.skill
                        )));
                    }
                }
            }
        };

        // Retry loop: up to 3 attempts (1 initial + 2 retries)
        const MAX_ATTEMPTS: usize = 3;
        let mut last_error: Option<AppError> = None;
        let task_id_str = task_id.to_string();

        for attempt in 0..MAX_ATTEMPTS {
            // Check cancellation before each attempt
            if token.is_cancelled() {
                return Err(AppError::Internal("Task cancelled".into()));
            }

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
                sandbox_handle,
                &task_id_str,
                &self.redis,
                0,
                &self.pool,
                Some(step.id),
            ).await;

            match result {
                Ok(agent_result) => {
                    let step_output = json!({"output": agent_result.output});
                    let artifacts = agent_result.artifacts.clone();

                    // Store artifacts in DB — read content from sandbox before it's released
                    for artifact_path in &agent_result.artifacts {
                        let name = artifact_path.rsplit('/').next().unwrap_or(artifact_path);
                        let artifact_type = Self::infer_artifact_type(name);
                        let mime = Self::infer_mime_type(name);
                        let content: Option<String> = sandbox_handle.read_file(artifact_path).await.ok();
                        let size = content.as_ref().map(|c| c.len() as i64);
                        let _ = db::create_artifact(
                            &self.pool, task_id, Some(step.id),
                            name, &artifact_type, Some(&mime),
                            Some(artifact_path), content.as_deref(), None, size,
                        ).await;
                    }

                    db::update_step_status(&self.pool, step.id, TaskStatus::Completed).await?;
                    self.emit(task_id, "step_completed", json!({
                        "step": step_index + 1,
                        "skill": step.skill,
                        "artifacts": agent_result.artifacts,
                        "result_preview": agent_result.output.chars().take(500).collect::<String>(),
                        "turns_used": agent_result.turns_used,
                    })).await;
                    db::add_task_event(&self.pool, task_id, "step_completed",
                        json!({"step": step_index + 1, "skill": step.skill})).await?;

                    // Update memory with latest result
                    db::set_memory(&self.pool, task_id, &format!("step_{}_result", step_index + 1),
                        step_output.clone()).await?;

                    return Ok((step_output, artifacts, agent_result.token_usage));
                }
                Err(e) => {
                    let err_str = e.to_string();
                    let _ = db::set_step_error(&self.pool, step.id, &err_str).await;

                    if attempt < MAX_ATTEMPTS - 1 {
                        let retry_count = db::increment_step_retry(&self.pool, step.id).await
                            .unwrap_or(attempt as i32 + 1);
                        let reflection = format!(
                            "Attempt {} failed: {}. Retrying with different approach.",
                            attempt + 1, err_str
                        );
                        let _ = db::set_step_reflection(&self.pool, step.id, &reflection).await;

                        self.emit(task_id, "step_retrying", json!({
                            "step": step_index + 1,
                            "attempt": attempt + 1,
                            "max_attempts": MAX_ATTEMPTS,
                            "retry_count": retry_count,
                            "error": err_str,
                        })).await;

                        last_error = Some(e);
                    } else {
                        last_error = Some(e);
                    }
                }
            }
        }

        // All retries exhausted
        let err = last_error.unwrap_or_else(|| AppError::Internal("Step failed".into()));
        db::update_step_status(&self.pool, step.id, TaskStatus::Failed).await?;
        self.emit(task_id, "step_failed", json!({
            "step": step_index + 1,
            "error": err.to_string(),
            "retries_exhausted": true,
        })).await;

        Err(err)
    }

    /// Write input artifacts into the sandbox /workspace/ directory.
    ///
    /// Artifact content is stored as base64 in the DB. To avoid shell
    /// command-line length limits for large files, we write the base64 text to
    /// a temporary file via SandboxHandle::write_file, then decode it with
    /// `base64 -d` into the final workspace path.
    async fn write_input_artifacts(
        sandbox: &SandboxHandle,
        artifacts: &[InputArtifact],
    ) -> Result<(), AppError> {
        for artifact in artifacts {
            let b64_content = artifact.content.as_deref().unwrap_or("");
            if b64_content.is_empty() {
                continue;
            }

            let dest = format!("/workspace/{}", artifact.name);
            let tmp = format!("/tmp/.input_{}.b64", artifact.name);

            // Write the base64 text to a temp file (write_file handles quoting safely)
            sandbox.write_file(&tmp, b64_content).await
                .map_err(|e| AppError::Internal(format!(
                    "Failed to stage input file '{}': {}", artifact.name, e
                )))?;

            // Decode from the temp file into the workspace path
            let cmd = format!("base64 -d {} > {}", tmp, dest);
            let result = sandbox.exec(&["bash", "-c", &cmd]).await
                .map_err(|e| AppError::Internal(format!(
                    "Failed to decode input file '{}': {}", artifact.name, e
                )))?;

            if result.exit_code != 0 {
                return Err(AppError::Internal(format!(
                    "base64 decode failed for '{}': {}", artifact.name, result.stderr
                )));
            }

            // Clean up temp file
            let _ = sandbox.exec(&["rm", "-f", &tmp]).await;
        }
        Ok(())
    }

    /// Create the initial execution plan
    async fn create_plan(
        &self,
        goal: &str,
        user_context: Option<&str>,
        input_artifacts: &[InputArtifact],
    ) -> Result<Vec<Value>, AppError> {
        let agent_list = self.agents.list();
        let agent_descriptions = agent_list.iter()
            .map(|(name, desc)| format!("- {name}: {desc}"))
            .collect::<Vec<_>>()
            .join("\n");

        let role_instructions = "You are a world-class autonomous AI agent planner. Your job is to decompose \
             complex goals into a precise sequence of executable steps, each delegated to a specialist agent.";

        let system = soul::system_prompt(role_instructions, user_context);

        // Build optional file list section for the planning prompt
        let file_list_section = if !input_artifacts.is_empty() {
            let file_list: String = input_artifacts
                .iter()
                .map(|a| format!(
                    "- {} ({}, {} bytes)",
                    a.name,
                    a.mime_type.as_deref().unwrap_or("unknown type"),
                    a.content.as_deref().unwrap_or("").len() * 3 / 4, // approx decoded size
                ))
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "\n\nUser-provided files (available in /workspace/ inside the sandbox):\n{file_list}\n"
            )
        } else {
            String::new()
        };

        let plan_prompt = format!(
            "Goal: {goal}{file_list_section}\n\n\
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

    /// Adaptive re-planning when a step fails (currently unused in parallel mode,
    /// retained for potential future sequential fallback).
    #[allow(dead_code)]
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_json_array ──────────────────────────

    #[test]
    fn parse_json_array_plain() {
        let input = r#"[{"skill": "code", "description": "write code"}]"#;
        let result = Orchestrator::parse_json_array(input).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["skill"], "code");
    }

    #[test]
    fn parse_json_array_with_markdown_fences() {
        let input = "```json\n[{\"skill\": \"browser\", \"description\": \"browse\"}]\n```";
        let result = Orchestrator::parse_json_array(input).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["skill"], "browser");
    }

    #[test]
    fn parse_json_array_with_whitespace() {
        let input = "  \n  [{\"skill\": \"research\"}]  \n  ";
        let result = Orchestrator::parse_json_array(input).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn parse_json_array_invalid_json() {
        let input = "not json at all";
        assert!(Orchestrator::parse_json_array(input).is_err());
    }

    #[test]
    fn parse_json_array_empty() {
        let result = Orchestrator::parse_json_array("[]").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_json_array_multi_step() {
        let input = r#"[
            {"skill": "research", "description": "find data", "depends_on": []},
            {"skill": "code", "description": "process data", "depends_on": [0]},
            {"skill": "deploy", "description": "deploy", "depends_on": [1]}
        ]"#;
        let result = Orchestrator::parse_json_array(input).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[1]["depends_on"][0], 0);
        assert_eq!(result[2]["depends_on"][0], 1);
    }

    // ── infer_artifact_type ───────────────────────

    #[test]
    fn infer_artifact_type_screenshot() {
        assert_eq!(Orchestrator::infer_artifact_type("result.png"), "screenshot");
        assert_eq!(Orchestrator::infer_artifact_type("PHOTO.JPG"), "screenshot");
    }

    #[test]
    fn infer_artifact_type_code() {
        assert_eq!(Orchestrator::infer_artifact_type("main.py"), "code");
        assert_eq!(Orchestrator::infer_artifact_type("index.ts"), "code");
    }

    #[test]
    fn infer_artifact_type_data() {
        assert_eq!(Orchestrator::infer_artifact_type("output.csv"), "data");
        assert_eq!(Orchestrator::infer_artifact_type("config.json"), "data");
    }

    #[test]
    fn infer_artifact_type_report() {
        assert_eq!(Orchestrator::infer_artifact_type("README.md"), "report");
    }

    #[test]
    fn infer_artifact_type_unknown() {
        assert_eq!(Orchestrator::infer_artifact_type("binary.dat"), "file");
    }

    // ── infer_mime_type ───────────────────────────

    #[test]
    fn infer_mime_type_common() {
        assert_eq!(Orchestrator::infer_mime_type("img.png"), "image/png");
        assert_eq!(Orchestrator::infer_mime_type("style.css"), "text/css");
        assert_eq!(Orchestrator::infer_mime_type("data.json"), "application/json");
        assert_eq!(Orchestrator::infer_mime_type("unknown.xyz"), "application/octet-stream");
    }

    // ── dependency graph logic ────────────────────

    #[test]
    fn dependency_graph_parsing() {
        let plan: Vec<Value> = vec![
            json!({"skill": "research", "depends_on": []}),
            json!({"skill": "code", "depends_on": [0]}),
            json!({"skill": "deploy", "depends_on": [0, 1]}),
        ];

        let deps: Vec<Vec<usize>> = plan.iter().map(|s| {
            s["depends_on"].as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_u64().map(|n| n as usize)).collect())
                .unwrap_or_default()
        }).collect();

        assert_eq!(deps[0], Vec::<usize>::new());
        assert_eq!(deps[1], vec![0]);
        assert_eq!(deps[2], vec![0, 1]);
    }

    #[test]
    fn ready_step_filter_logic() {
        let deps = vec![
            vec![],       // step 0: no deps
            vec![0],      // step 1: depends on 0
            vec![0, 1],   // step 2: depends on 0 and 1
        ];
        let total_steps = 3;

        // Initially: only step 0 is ready
        let completed: HashSet<usize> = HashSet::new();
        let failed: HashSet<usize> = HashSet::new();
        let ready: Vec<usize> = (0..total_steps)
            .filter(|i| !completed.contains(i) && !failed.contains(i))
            .filter(|i| !deps[*i].iter().any(|d| failed.contains(d)))
            .filter(|i| deps[*i].iter().all(|d| completed.contains(d)))
            .collect();
        assert_eq!(ready, vec![0]);

        // After step 0 completes: step 1 is ready
        let mut completed: HashSet<usize> = HashSet::new();
        completed.insert(0);
        let failed: HashSet<usize> = HashSet::new();
        let ready: Vec<usize> = (0..total_steps)
            .filter(|i| !completed.contains(i) && !failed.contains(i))
            .filter(|i| !deps[*i].iter().any(|d| failed.contains(d)))
            .filter(|i| deps[*i].iter().all(|d| completed.contains(d)))
            .collect();
        assert_eq!(ready, vec![1]);

        // If step 0 fails: steps 1 and 2 should NOT be ready (blocked by failed dep)
        let completed: HashSet<usize> = HashSet::new();
        let mut failed: HashSet<usize> = HashSet::new();
        failed.insert(0);
        let ready: Vec<usize> = (0..total_steps)
            .filter(|i| !completed.contains(i) && !failed.contains(i))
            .filter(|i| !deps[*i].iter().any(|d| failed.contains(d)))
            .filter(|i| deps[*i].iter().all(|d| completed.contains(d)))
            .collect();
        assert!(ready.is_empty(), "No steps should be ready when dep 0 failed");
    }

    #[test]
    fn parallel_independent_steps() {
        let deps = vec![
            vec![],  // step 0: no deps
            vec![],  // step 1: no deps
            vec![],  // step 2: no deps
        ];
        let total_steps = 3;
        let completed: HashSet<usize> = HashSet::new();
        let failed: HashSet<usize> = HashSet::new();

        let ready: Vec<usize> = (0..total_steps)
            .filter(|i| !completed.contains(i) && !failed.contains(i))
            .filter(|i| !deps[*i].iter().any(|d| failed.contains(d)))
            .filter(|i| deps[*i].iter().all(|d| completed.contains(d)))
            .collect();

        // All 3 steps should be ready in parallel
        assert_eq!(ready, vec![0, 1, 2]);
    }
}

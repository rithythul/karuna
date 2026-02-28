use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

use crate::db;
use crate::error::AppError;
use crate::llm::{ChatMessage, LlmClient};
use crate::models::TaskStatus;
use crate::redis_client::{RedisClient, TaskEvent};
use crate::sandbox::SandboxManager;
use crate::skills::{SkillContext, SkillRegistry};

const MAX_STEP_RETRIES: u32 = 3;

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

    /// Execute a single task end-to-end with self-reflection and adaptive re-planning
    async fn execute_task(&self, task_id: Uuid) -> Result<(), AppError> {
        let start = Instant::now();
        let task = db::get_task(&self.pool, task_id).await?;
        self.emit(task_id, "task_started", json!({"goal": task.goal})).await;

        // Phase 1: Plan
        db::update_task_status(&self.pool, task_id, TaskStatus::Planning).await?;
        self.emit(task_id, "planning", json!({})).await;

        let plan = self.create_plan(&task.goal).await?;

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

        for (i, step) in steps.iter().enumerate() {
            self.emit(task_id, "step_started", json!({
                "step": i + 1,
                "total": steps.len(),
                "skill": step.skill,
                "description": step.description,
            })).await;

            db::update_step_status(&self.pool, step.id, TaskStatus::Running).await?;

            let skill = match self.skills.get(&step.skill) {
                Some(s) => s,
                None => {
                    // Fallback: use shell skill for unknown skills
                    tracing::warn!("Unknown skill '{}', attempting shell fallback", step.skill);
                    self.emit(task_id, "step_warning", json!({
                        "step": i + 1,
                        "warning": format!("Unknown skill '{}', using shell fallback", step.skill),
                    })).await;
                    match self.skills.get("shell") {
                        Some(s) => s,
                        None => {
                            db::update_step_status(&self.pool, step.id, TaskStatus::Failed).await?;
                            continue;
                        }
                    }
                }
            };

            let ctx = SkillContext {
                llm: self.llm.clone(),
                sandbox: self.sandbox.clone(),
                container_id: container_id.clone(),
                task_id: task_id.to_string(),
            };

            // Build input with context from previous steps
            let mut input = current_plan.get(i)
                .and_then(|s| s.get("input"))
                .cloned()
                .unwrap_or(json!({}));

            if let Some(obj) = input.as_object_mut() {
                obj.insert("_previous_result".into(), step_results.last().cloned().unwrap_or(json!({})));
                obj.insert("_all_previous_results".into(), json!(step_results));
                obj.insert("_step_number".into(), json!(i + 1));
                obj.insert("_total_steps".into(), json!(steps.len()));
            }

            // Execute with self-healing retry loop
            let result = self.execute_step_with_reflection(
                &ctx, task_id, step.id, &skill, input, i, &step.skill, &step.description,
            ).await;

            match result {
                Ok(output) => {
                    step_results.push(output.result.clone());
                    all_artifacts.extend(output.artifacts.clone());

                    // Store artifacts in DB
                    for artifact_path in &output.artifacts {
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
                        "artifacts": output.artifacts,
                        "result_preview": output.result.to_string().chars().take(500).collect::<String>(),
                    })).await;
                    db::add_task_event(&self.pool, task_id, "step_completed",
                        json!({"step": i + 1, "skill": step.skill})).await?;

                    // Update memory with latest result
                    db::set_memory(&self.pool, task_id, &format!("step_{}_result", i + 1),
                        output.result).await?;
                }
                Err(e) => {
                    db::update_step_status(&self.pool, step.id, TaskStatus::Failed).await?;
                    self.emit(task_id, "step_failed", json!({
                        "step": i + 1,
                        "error": e.to_string(),
                    })).await;

                    // Try adaptive re-planning for remaining steps
                    if i < steps.len() - 1 {
                        self.emit(task_id, "replanning", json!({
                            "reason": format!("Step {} ({}) failed: {}", i + 1, step.skill, e),
                        })).await;

                        match self.replan(
                            &task.goal, &current_plan, &step_results, i, &e.to_string(),
                        ).await {
                            Ok(_new_plan) => {
                                self.emit(task_id, "replan_ready", json!({
                                    "new_steps": _new_plan.len(),
                                })).await;
                                continue;
                            }
                            Err(replan_err) => {
                                tracing::error!("Re-planning failed: {replan_err}");
                            }
                        }
                    }

                    db::set_task_error(&self.pool, task_id, &e.to_string()).await?;
                    let _ = self.sandbox.release(&container_id).await;
                    let duration = start.elapsed().as_millis() as i64;
                    let _ = db::set_task_duration(&self.pool, task_id, duration).await;
                    self.emit(task_id, "task_failed", json!({"error": e.to_string()})).await;
                    return Err(e);
                }
            }
        }

        // Phase 3: Synthesize final result
        let final_result = self.synthesize_result(
            &task.goal, &step_results, &all_artifacts,
        ).await?;

        db::set_task_result(&self.pool, task_id, final_result.clone()).await?;

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
    async fn create_plan(&self, goal: &str) -> Result<Vec<Value>, AppError> {
        let skill_list = self.skills.list_with_schema();
        let skill_descriptions = skill_list.iter()
            .map(|(name, desc, schema)| format!("- {name}: {desc}\n  Input schema: {schema}"))
            .collect::<Vec<_>>()
            .join("\n");

        let plan_prompt = format!(
            "You are a world-class autonomous AI agent planner. Your job is to decompose \
             complex goals into a precise sequence of executable steps.\n\n\
             Goal: {goal}\n\n\
             Available skills:\n{skill_descriptions}\n\n\
             PLANNING GUIDELINES:\n\
             1. Break the goal into the MINIMUM number of steps needed\n\
             2. Use the most specific skill for each subtask\n\
             3. Steps execute sequentially — later steps can reference earlier results\n\
             4. For web tasks, use 'browse' skill with Playwright\n\
             5. For file creation/editing, use 'file' skill\n\
             6. For data processing, use 'data_analysis' skill\n\
             7. For running tools/builds, use 'shell' skill\n\
             8. For research/information gathering, use 'research' skill\n\
             9. For coding tasks, use 'code' skill\n\
             10. For deployment, use 'deploy' skill as the final step\n\n\
             Return a JSON array of steps. Each step has:\n\
             - \"skill\": skill name (MUST be one from above)\n\
             - \"description\": what this step accomplishes\n\
             - \"input\": input object matching the skill's schema EXACTLY\n\n\
             IMPORTANT:\n\
             - Use EXACT field names from each skill's input schema\n\
             - For 'code' skill: {{\"task\": \"description\", \"language\": \"python\"}}\n\
             - For 'research' skill: {{\"query\": \"what to research\"}}\n\
             - For 'browse' skill: {{\"task\": \"what to do\", \"url\": \"optional url\"}}\n\
             - For 'file' skill: {{\"operation\": \"create|read|edit|list\", \"path\": \"...\", \"content\": \"...\"}}\n\
             - For 'data_analysis' skill: {{\"task\": \"what to analyze\"}}\n\
             - For 'shell' skill: {{\"command\": \"shell command\"}}\n\
             - For 'deploy' skill: {{\"task\": \"what to deploy\"}}\n\n\
             Return ONLY valid JSON array, no markdown fences, no explanation."
        );

        let plan_response = self.llm.plan(vec![
            ChatMessage { role: "user".into(), content: plan_prompt },
        ]).await?;

        Self::parse_json_array(&plan_response)
    }

    /// Execute a single step with self-reflection and retry
    async fn execute_step_with_reflection(
        &self,
        ctx: &SkillContext,
        task_id: Uuid,
        step_id: Uuid,
        skill: &Arc<dyn crate::skills::Skill>,
        mut input: Value,
        step_index: usize,
        skill_name: &str,
        description: &str,
    ) -> Result<crate::skills::SkillOutput, AppError> {
        let mut attempt = 0u32;

        loop {
            match skill.execute(ctx, input.clone()).await {
                Ok(output) if output.success => return Ok(output),
                Ok(output) => {
                    // Skill returned success=false (soft failure)
                    attempt += 1;
                    if attempt >= MAX_STEP_RETRIES {
                        return Ok(output);
                    }

                    // Self-reflection: ask LLM what went wrong
                    let reflection = self.reflect_on_failure(
                        skill_name, description, &input, &output.result,
                    ).await?;

                    tracing::info!(
                        task_id = ctx.task_id.as_str(),
                        step = step_index + 1,
                        attempt,
                        "Self-reflection: {}",
                        reflection.chars().take(200).collect::<String>()
                    );

                    self.emit(task_id, "step_reflection", json!({
                        "step": step_index + 1,
                        "attempt": attempt,
                        "reflection": reflection.chars().take(500).collect::<String>(),
                    })).await;

                    db::set_step_reflection(&self.pool, step_id, &reflection, attempt as i32).await?;

                    if let Some(obj) = input.as_object_mut() {
                        obj.insert("_reflection".into(), json!(reflection));
                        obj.insert("_retry_attempt".into(), json!(attempt));
                        obj.insert("_previous_error".into(), output.result.clone());
                    }
                }
                Err(e) => {
                    attempt += 1;
                    if attempt >= MAX_STEP_RETRIES {
                        return Err(e);
                    }

                    let reflection = self.reflect_on_error(
                        skill_name, description, &input, &e.to_string(),
                    ).await.unwrap_or_else(|_| "Unable to reflect on error".to_string());

                    self.emit(task_id, "step_reflection", json!({
                        "step": step_index + 1,
                        "attempt": attempt,
                        "error": e.to_string(),
                        "reflection": reflection.chars().take(500).collect::<String>(),
                    })).await;

                    db::set_step_reflection(&self.pool, step_id, &reflection, attempt as i32).await?;

                    if let Some(obj) = input.as_object_mut() {
                        obj.insert("_reflection".into(), json!(reflection));
                        obj.insert("_retry_attempt".into(), json!(attempt));
                        obj.insert("_previous_error".into(), json!(e.to_string()));
                    }
                }
            }
        }
    }

    /// Self-reflection: analyze why a step produced a soft failure
    async fn reflect_on_failure(
        &self,
        skill_name: &str,
        description: &str,
        input: &Value,
        output: &Value,
    ) -> Result<String, AppError> {
        let messages = vec![
            ChatMessage {
                role: "system".into(),
                content: "You are a self-reflective AI agent analyzing a failed step. \
                    Identify what went wrong and suggest a concrete fix. Be brief and actionable.".into(),
            },
            ChatMessage {
                role: "user".into(),
                content: format!(
                    "Skill: {skill_name}\nDescription: {description}\n\
                     Input: {}\nOutput (failure): {}\n\n\
                     What went wrong and how should we fix it?",
                    serde_json::to_string_pretty(input).unwrap_or_default(),
                    serde_json::to_string_pretty(output).unwrap_or_default(),
                ),
            },
        ];
        self.llm.fast(messages).await
    }

    /// Self-reflection: analyze why a step threw an error
    async fn reflect_on_error(
        &self,
        skill_name: &str,
        description: &str,
        input: &Value,
        error: &str,
    ) -> Result<String, AppError> {
        let messages = vec![
            ChatMessage {
                role: "system".into(),
                content: "You are a self-reflective AI agent analyzing a step error. \
                    Identify the root cause and suggest a concrete fix. Be brief.".into(),
            },
            ChatMessage {
                role: "user".into(),
                content: format!(
                    "Skill: {skill_name}\nDescription: {description}\n\
                     Input: {}\nError: {error}\n\n\
                     Root cause and fix?",
                    serde_json::to_string_pretty(input).unwrap_or_default(),
                ),
            },
        ];
        self.llm.fast(messages).await
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

        let skill_list = self.skills.list_with_schema();
        let skill_descriptions = skill_list.iter()
            .map(|(name, desc, schema)| format!("- {name}: {desc}\n  Input schema: {schema}"))
            .collect::<Vec<_>>()
            .join("\n");

        let messages = vec![
            ChatMessage {
                role: "system".into(),
                content: "You are an adaptive re-planner. A step failed during task execution. \
                    Create a NEW plan for the REMAINING work, taking into account what's already done.\n\
                    Return ONLY a JSON array of new steps (same format as original plan).\n\
                    Try a different approach for the failed step.".into(),
            },
            ChatMessage {
                role: "user".into(),
                content: format!(
                    "Goal: {goal}\n\n\
                     Completed steps:\n{completed}\n\n\
                     FAILED step: {failed_step_desc}\nError: {error}\n\n\
                     Remaining planned steps:\n{remaining}\n\n\
                     Available skills:\n{skill_descriptions}\n\n\
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

        let messages = vec![
            ChatMessage {
                role: "system".into(),
                content: "You are a result synthesizer. Create a clear, structured summary of \
                    what was accomplished. Return a JSON object with:\n\
                    {\"summary\": \"brief overview\", \"key_outputs\": [\"...\"], \"artifacts\": [\"...\"], \"next_steps\": [\"...\"]}".into(),
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
        let parsed: Value = serde_json::from_str(response.trim())
            .unwrap_or_else(|_| json!({
                "summary": response,
                "key_outputs": step_results.to_vec(),
                "artifacts": artifacts,
            }));
        Ok(parsed)
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

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::{error, info, warn};

use tokio::time::timeout;

use crate::error::AppError;
use crate::llm::{LlmClient, LlmResponse, ToolDefinition};
use crate::redis_client::{RedisClient, TaskEvent};
use crate::tools::{AgentTool, SandboxHandle};

// ---------------------------------------------------------------------------
// AgentResult — the output of an agent run
// ---------------------------------------------------------------------------

/// The final result produced by an agent after completing its think-act-observe loop.
pub struct AgentResult {
    /// The agent's final textual output.
    pub output: String,
    /// Paths or identifiers of artifacts produced during execution.
    pub artifacts: Vec<String>,
    /// Number of think-act-observe turns consumed.
    pub turns_used: u32,
}

// ---------------------------------------------------------------------------
// Agent trait — defines what an agent can do
// ---------------------------------------------------------------------------

/// An autonomous agent that can reason and use tools to accomplish goals.
///
/// Each agent has a name, description, system prompt, and a set of tools it can
/// use. The `AgentRuntime` drives the think-act-observe loop for any type that
/// implements this trait.
#[async_trait]
pub trait Agent: Send + Sync {
    /// Machine-readable name for this agent (e.g. "coder", "researcher").
    fn name(&self) -> &str;

    /// Short human-readable description of the agent's purpose.
    fn description(&self) -> &str;

    /// The system prompt that defines this agent's personality and capabilities.
    fn system_prompt(&self) -> String;

    /// The set of tools available to this agent.
    fn tools(&self) -> Vec<Arc<dyn AgentTool>>;

    /// Maximum number of think-act-observe turns before forcing a summary.
    fn max_turns(&self) -> u32 {
        20
    }
}

// ---------------------------------------------------------------------------
// AgentRegistry — a registry of named agents
// ---------------------------------------------------------------------------

/// Registry that maps agent names to agent instances.
///
/// Used by the `AgentRuntime` to look up agents for delegation.
pub struct AgentRegistry {
    agents: HashMap<String, Arc<dyn Agent>>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
        }
    }

    /// Register an agent. The agent's `name()` is used as the key.
    pub fn register(&mut self, agent: Arc<dyn Agent>) {
        let name = agent.name().to_string();
        self.agents.insert(name, agent);
    }

    /// Look up an agent by name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn Agent>> {
        self.agents.get(name).cloned()
    }

    /// List all registered agents as `(name, description)` pairs.
    pub fn list(&self) -> Vec<(&str, &str)> {
        self.agents
            .values()
            .map(|a| (a.name(), a.description()))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// AgentRuntime — the think-act-observe loop engine
// ---------------------------------------------------------------------------

/// Drives the think-act-observe loop for any `Agent`.
///
/// The runtime calls the LLM with the agent's tools, executes tool calls,
/// pushes observations back into the conversation, and repeats until the LLM
/// returns a final text response or the turn limit is reached.
///
/// Supports delegation: if the LLM calls a special "delegate" tool, the runtime
/// looks up a child agent in the registry and runs it recursively (up to a
/// configurable max depth).
pub struct AgentRuntime {
    pub llm: LlmClient,
    pub registry: Arc<AgentRegistry>,
}

impl AgentRuntime {
    pub fn new(llm: LlmClient, registry: Arc<AgentRegistry>) -> Self {
        Self { llm, registry }
    }

    /// Run the think-act-observe loop for the given agent.
    ///
    /// # Arguments
    /// * `agent` - The agent to run.
    /// * `goal` - The user's goal / task description.
    /// * `sandbox` - The sandbox handle for tool execution.
    /// * `task_id` - The task ID (for event emission).
    /// * `redis` - Redis client for publishing WebSocket events.
    /// * `depth` - Current delegation depth (starts at 0).
    ///
    /// The future is boxed to support recursive delegation (async fn cannot be
    /// directly recursive without indirection).
    pub fn run<'a>(
        &'a self,
        agent: &'a Arc<dyn Agent>,
        goal: &'a str,
        sandbox: &'a SandboxHandle,
        task_id: &'a str,
        redis: &'a RedisClient,
        depth: u32,
    ) -> Pin<Box<dyn Future<Output = Result<AgentResult, AppError>> + Send + 'a>> {
        Box::pin(async move {
        let agent_name = agent.name().to_string();

        // Emit agent_started event
        self.emit(redis, task_id, "agent_started", json!({
            "agent": &agent_name,
            "goal": goal,
            "depth": depth,
        }))
        .await;

        // Build tool definitions from the agent's tools
        let agent_tools = agent.tools();
        let tool_defs: Vec<ToolDefinition> = agent_tools
            .iter()
            .map(|t| t.to_tool_definition())
            .collect();

        // Initialize conversation
        let mut messages: Vec<Value> = vec![
            json!({"role": "system", "content": agent.system_prompt()}),
            json!({"role": "user", "content": goal}),
        ];

        let mut artifacts: Vec<String> = Vec::new();
        let mut turns: u32 = 0;
        let model = &self.llm.default_model.clone();

        loop {
            // THINK: call the LLM with tools
            let response = self
                .llm
                .chat_with_tools(
                    model,
                    messages.clone(),
                    Some(tool_defs.clone()),
                    Some(0.2),
                    Some(4096),
                )
                .await;

            let response = match response {
                Ok(r) => r,
                Err(e) => {
                    self.emit(redis, task_id, "agent_error", json!({
                        "agent": &agent_name,
                        "error": e.to_string(),
                        "turn": turns,
                    }))
                    .await;
                    return Err(e);
                }
            };

            match response {
                LlmResponse::Text(text) => {
                    // DONE: the agent produced a final answer
                    let output_preview: String = text.chars().take(500).collect();
                    self.emit(redis, task_id, "agent_completed", json!({
                        "agent": &agent_name,
                        "output_preview": output_preview,
                        "turns_used": turns,
                        "artifacts": &artifacts,
                    }))
                    .await;

                    return Ok(AgentResult {
                        output: text,
                        artifacts,
                        turns_used: turns,
                    });
                }

                LlmResponse::ToolCalls(tool_calls) => {
                    // Build the assistant message with tool_calls (must come before
                    // tool result messages per OpenAI format).
                    let tc_values: Vec<Value> = tool_calls
                        .iter()
                        .map(|tc| {
                            json!({
                                "id": tc.id,
                                "type": tc.call_type,
                                "function": {
                                    "name": tc.function.name,
                                    "arguments": tc.function.arguments,
                                }
                            })
                        })
                        .collect();

                    messages.push(json!({
                        "role": "assistant",
                        "tool_calls": tc_values,
                        "content": null
                    }));

                    // Process each tool call
                    for tc in &tool_calls {
                        let tool_name = &tc.function.name;
                        let tool_call_id = &tc.id;

                        if tool_name == "delegate" {
                            // Parse delegate args for the event
                            let delegate_args: Value =
                                serde_json::from_str(&tc.function.arguments)
                                    .unwrap_or(json!({}));
                            let child_agent_name = delegate_args["agent"]
                                .as_str()
                                .unwrap_or("unknown");
                            let sub_goal_preview = delegate_args["goal"]
                                .as_str()
                                .unwrap_or("unknown");

                            // Handle delegation to a child agent
                            self.emit(redis, task_id, "agent_delegating", json!({
                                "agent": &agent_name,
                                "child_agent": child_agent_name,
                                "sub_goal": sub_goal_preview,
                            }))
                            .await;

                            match self
                                .handle_delegation(tc, sandbox, task_id, redis, depth)
                                .await
                            {
                                Ok(result) => {
                                    artifacts.extend(result.artifacts.clone());

                                    messages.push(json!({
                                        "role": "tool",
                                        "tool_call_id": tool_call_id,
                                        "content": serde_json::to_string(&json!({
                                            "output": result.output,
                                            "artifacts": result.artifacts,
                                        })).unwrap_or_default()
                                    }));
                                }
                                Err(e) => {
                                    warn!(
                                        agent = agent_name,
                                        tool = "delegate",
                                        error = %e,
                                        "Delegation failed"
                                    );
                                    messages.push(json!({
                                        "role": "tool",
                                        "tool_call_id": tool_call_id,
                                        "content": format!("Delegation failed: {e}")
                                    }));
                                }
                            }
                        } else {
                            // ACT: execute the tool
                            let params: Value =
                                serde_json::from_str(&tc.function.arguments).unwrap_or(json!({}));

                            // Find the tool
                            let tool = agent_tools.iter().find(|t| t.name() == tool_name);

                            match tool {
                                Some(tool) => {
                                    let display = tool.display_message(&params);

                                    self.emit(redis, task_id, "agent_tool_call", json!({
                                        "agent": &agent_name,
                                        "tool": tool_name,
                                        "display": &display,
                                    }))
                                    .await;

                                    let tool_timeout = Self::tool_timeout(tool_name);
                                    let exec_result =
                                        timeout(tool_timeout, tool.execute(params, sandbox)).await;

                                    match exec_result {
                                        Ok(Ok(result)) => {
                                            self.emit(
                                                redis,
                                                task_id,
                                                "agent_tool_result",
                                                json!({
                                                    "agent": &agent_name,
                                                    "tool": tool_name,
                                                    "display": &result.display,
                                                    "artifacts": &result.artifacts,
                                                }),
                                            )
                                            .await;

                                            artifacts.extend(result.artifacts);

                                            // OBSERVE: push tool result message
                                            let output_str = serde_json::to_string(&result.output)
                                                .unwrap_or_default();
                                            messages.push(json!({
                                                "role": "tool",
                                                "tool_call_id": tool_call_id,
                                                "content": output_str
                                            }));
                                        }
                                        Ok(Err(e)) => {
                                            self.emit(
                                                redis,
                                                task_id,
                                                "agent_error",
                                                json!({
                                                    "agent": &agent_name,
                                                    "error": e.to_string(),
                                                    "turn": turns,
                                                }),
                                            )
                                            .await;

                                            messages.push(json!({
                                                "role": "tool",
                                                "tool_call_id": tool_call_id,
                                                "content": format!("Tool error: {e}")
                                            }));
                                        }
                                        Err(_elapsed) => {
                                            warn!(
                                                agent = agent_name,
                                                tool = tool_name.as_str(),
                                                timeout_secs = tool_timeout.as_secs(),
                                                "Tool execution timed out"
                                            );

                                            self.emit(
                                                redis,
                                                task_id,
                                                "agent_error",
                                                json!({
                                                    "agent": &agent_name,
                                                    "error": format!(
                                                        "Tool '{}' timed out after {}s",
                                                        tool_name,
                                                        tool_timeout.as_secs()
                                                    ),
                                                    "turn": turns,
                                                }),
                                            )
                                            .await;

                                            messages.push(json!({
                                                "role": "tool",
                                                "tool_call_id": tool_call_id,
                                                "content": format!(
                                                    "Tool '{}' timed out after {}s. Try a different approach.",
                                                    tool_name,
                                                    tool_timeout.as_secs()
                                                )
                                            }));
                                        }
                                    }
                                }
                                None => {
                                    warn!(
                                        agent = agent_name,
                                        tool = tool_name.as_str(),
                                        "Unknown tool requested by agent"
                                    );
                                    messages.push(json!({
                                        "role": "tool",
                                        "tool_call_id": tool_call_id,
                                        "content": format!("Unknown tool: {tool_name}")
                                    }));
                                }
                            }
                        }
                    }

                    turns += 1;

                    // Enforce max turns limit
                    if turns >= agent.max_turns() {
                        info!(
                            agent = agent_name,
                            turns,
                            "Agent reached max turns limit, forcing summary"
                        );

                        messages.push(json!({
                            "role": "user",
                            "content": "Turn limit reached. Summarize your work so far."
                        }));

                        // Call LLM without tools to force a text response
                        let final_response = self
                            .llm
                            .chat_with_tools(
                                model,
                                messages,
                                None, // no tools -> forces text response
                                Some(0.2),
                                Some(4096),
                            )
                            .await;

                        let output = match final_response {
                            Ok(LlmResponse::Text(text)) => text,
                            Ok(LlmResponse::ToolCalls(_)) => {
                                "Agent reached turn limit and could not produce a summary."
                                    .to_string()
                            }
                            Err(e) => format!("Agent reached turn limit. Final call failed: {e}"),
                        };

                        let output_preview: String = output.chars().take(500).collect();
                        self.emit(redis, task_id, "agent_completed", json!({
                            "agent": &agent_name,
                            "output_preview": output_preview,
                            "turns_used": turns,
                            "artifacts": &artifacts,
                        }))
                        .await;

                        return Ok(AgentResult {
                            output,
                            artifacts,
                            turns_used: turns,
                        });
                    }
                }
            }
        }
        }) // end Box::pin(async move { ... })
    }

    /// Handle delegation to a child agent.
    ///
    /// Parses the delegate tool call arguments to find the child agent name and
    /// sub-goal, looks up the child in the registry, and runs it recursively
    /// with `depth + 1`. Delegation is limited to a max depth of 3.
    async fn handle_delegation(
        &self,
        tool_call: &crate::llm::ToolCall,
        sandbox: &SandboxHandle,
        task_id: &str,
        redis: &RedisClient,
        depth: u32,
    ) -> Result<AgentResult, AppError> {
        // Enforce max delegation depth
        if depth >= 3 {
            return Err(AppError::Internal(
                "Maximum delegation depth (3) reached. Cannot delegate further.".to_string(),
            ));
        }

        // Parse the delegation arguments
        let args: Value =
            serde_json::from_str(&tool_call.function.arguments).map_err(|e| {
                AppError::BadRequest(format!("Invalid delegate arguments: {e}"))
            })?;

        let child_name = args["agent"]
            .as_str()
            .ok_or_else(|| AppError::BadRequest("delegate tool requires 'agent' field".into()))?;

        let sub_goal = args["goal"]
            .as_str()
            .ok_or_else(|| AppError::BadRequest("delegate tool requires 'goal' field".into()))?;

        // Look up the child agent
        let child_agent = self.registry.get(child_name).ok_or_else(|| {
            AppError::NotFound(format!("Agent '{child_name}' not found in registry"))
        })?;

        info!(
            parent_depth = depth,
            child_agent = child_name,
            sub_goal,
            "Delegating to child agent"
        );

        // Run the child agent recursively with the same sandbox (shared container)
        self.run(&child_agent, sub_goal, sandbox, task_id, redis, depth + 1)
            .await
    }

    /// Return the execution timeout for a given tool name.
    ///
    /// Browser tools get 120s (pages may be slow to load/render), code execution
    /// gets 300s (user programs can legitimately run for minutes), and everything
    /// else gets 60s.
    fn tool_timeout(tool_name: &str) -> Duration {
        match tool_name {
            "navigate" | "click" | "fill" | "extract" | "snapshot" | "screenshot" => {
                Duration::from_secs(120)
            }
            "run_code" => Duration::from_secs(300),
            _ => Duration::from_secs(60),
        }
    }

    /// Publish an event via Redis, logging errors instead of crashing the loop.
    async fn emit(&self, redis: &RedisClient, task_id: &str, event_type: &str, data: Value) {
        let event = TaskEvent {
            task_id: task_id.to_string(),
            event_type: event_type.to_string(),
            data,
        };
        if let Err(e) = redis.publish_event(&event).await {
            error!(event_type, error = %e, "Failed to publish agent event");
        }
    }
}

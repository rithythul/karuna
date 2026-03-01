use async_trait::async_trait;
use serde_json::{json, Value};

use crate::error::AppError;

use super::{AgentTool, SandboxHandle, ToolResult};

/// Tool that delegates a sub-goal to a specialized agent.
///
/// Delegation is intercepted by `AgentRuntime` before `execute()` is reached,
/// so the `execute()` implementation is a safety fallback that always errors.
pub struct DelegateTool {
    /// (name, description) pairs for each available agent.
    available_agents: Vec<(String, String)>,
}

impl DelegateTool {
    pub fn new(agents: Vec<(String, String)>) -> Self {
        Self {
            available_agents: agents,
        }
    }
}

#[async_trait]
impl AgentTool for DelegateTool {
    fn name(&self) -> &str {
        "delegate"
    }

    fn description(&self) -> &str {
        "Delegate a sub-goal to a specialized agent"
    }

    fn parameters_schema(&self) -> Value {
        let agent_names: Vec<&str> = self
            .available_agents
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();

        let agent_descriptions: Vec<String> = self
            .available_agents
            .iter()
            .map(|(name, desc)| format!("{name}: {desc}"))
            .collect();

        let description = format!(
            "The agent to delegate to. Available agents:\n{}",
            agent_descriptions.join("\n")
        );

        json!({
            "type": "object",
            "properties": {
                "agent": {
                    "type": "string",
                    "enum": agent_names,
                    "description": description
                },
                "goal": {
                    "type": "string",
                    "description": "A clear description of the sub-goal to delegate"
                }
            },
            "required": ["agent", "goal"]
        })
    }

    fn display_message(&self, params: &Value) -> String {
        let agent = params
            .get("agent")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        format!("Delegating to {agent} agent")
    }

    async fn execute(
        &self,
        _params: Value,
        _sandbox: &SandboxHandle,
    ) -> Result<ToolResult, AppError> {
        // This should never be reached — AgentRuntime intercepts delegate
        // tool calls before execute() is called.
        Err(AppError::Internal(
            "delegate tool is handled by AgentRuntime".to_string(),
        ))
    }
}

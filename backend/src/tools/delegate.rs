use async_trait::async_trait;
use serde_json::{json, Value};

use crate::error::AppError;

use super::{AgentTool, SandboxHandle, ToolResult};

/// Tool that delegates a sub-task to another specialist agent.
///
/// The agent runtime intercepts "delegate" tool calls before `execute()` is
/// reached (see `AgentRuntime::handle_delegation`), so the `execute` method
/// here is only a safety fallback that should never actually run.
pub struct DelegateTool;

#[async_trait]
impl AgentTool for DelegateTool {
    fn name(&self) -> &str {
        "delegate"
    }

    fn description(&self) -> &str {
        "Delegate a sub-task to another specialist agent. Use when the current \
         task requires expertise outside your specialty."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent": {
                    "type": "string",
                    "description": "Name of the specialist agent to delegate to (e.g. 'browser', 'code', 'research', 'api')"
                },
                "goal": {
                    "type": "string",
                    "description": "Clear description of the sub-goal for the delegated agent"
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
        let goal = params
            .get("goal")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        format!("Delegating to {agent}: {goal}")
    }

    async fn execute(
        &self,
        _params: Value,
        _sandbox: &SandboxHandle,
    ) -> Result<ToolResult, AppError> {
        // This should never be reached — the agent runtime intercepts "delegate"
        // tool calls and handles them via `handle_delegation()` before calling
        // `execute()`. If we get here, something is wrong.
        Err(AppError::Internal(
            "delegate tool should be handled by the agent runtime, not executed directly".into(),
        ))
    }
}

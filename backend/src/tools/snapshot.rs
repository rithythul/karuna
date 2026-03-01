use async_trait::async_trait;
use serde_json::{json, Value};

use crate::error::AppError;

use super::{AgentTool, SandboxHandle, ToolResult};

/// Tool that captures the current page accessibility snapshot.
pub struct SnapshotTool;

#[async_trait]
impl AgentTool for SnapshotTool {
    fn name(&self) -> &str {
        "snapshot"
    }

    fn description(&self) -> &str {
        "Capture the current page accessibility snapshot (ARIA tree). \
         Returns a structured text representation of the page content."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn display_message(&self, _params: &Value) -> String {
        "Reading page content".to_string()
    }

    async fn execute(
        &self,
        _params: Value,
        sandbox: &SandboxHandle,
    ) -> Result<ToolResult, AppError> {
        let result = sandbox.exec(&["agent-browser", "snapshot"]).await?;

        if result.exit_code != 0 {
            return Ok(ToolResult {
                output: json!({
                    "error": format!("Snapshot failed: {}", result.stderr.trim()),
                    "exit_code": result.exit_code,
                }),
                artifacts: Vec::new(),
                display: format!("Failed to capture snapshot: {}", result.stderr.trim()),
            });
        }

        Ok(ToolResult {
            output: json!({
                "snapshot": result.stdout,
            }),
            artifacts: Vec::new(),
            display: "Captured page snapshot".to_string(),
        })
    }
}

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::error::AppError;

use super::{AgentTool, SandboxHandle, ToolResult};

/// Tool that navigates to a URL in the sandbox browser.
pub struct NavigateTool;

#[async_trait]
impl AgentTool for NavigateTool {
    fn name(&self) -> &str {
        "navigate"
    }

    fn description(&self) -> &str {
        "Navigate to a URL in the browser. Returns the page accessibility snapshot."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to navigate to"
                }
            },
            "required": ["url"]
        })
    }

    fn display_message(&self, params: &Value) -> String {
        let url = params
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        format!("Opening {url}")
    }

    async fn execute(
        &self,
        params: Value,
        sandbox: &SandboxHandle,
    ) -> Result<ToolResult, AppError> {
        let url = params
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::BadRequest("navigate: missing 'url' parameter".into()))?;

        let result = sandbox
            .exec(&["agent-browser", "navigate", url])
            .await?;

        if result.exit_code != 0 {
            return Ok(ToolResult {
                output: json!({
                    "error": format!("Navigation failed: {}", result.stderr.trim()),
                    "exit_code": result.exit_code,
                }),
                artifacts: Vec::new(),
                display: format!("Failed to navigate to {url}: {}", result.stderr.trim()),
            });
        }

        Ok(ToolResult {
            output: json!({
                "url": url,
                "snapshot": result.stdout,
            }),
            artifacts: Vec::new(),
            display: format!("Navigated to {url}"),
        })
    }
}

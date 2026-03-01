use async_trait::async_trait;
use serde_json::{json, Value};

use crate::error::AppError;

use super::{AgentTool, SandboxHandle, ToolResult};

/// Tool that takes a screenshot of the current browser page.
pub struct ScreenshotTool;

#[async_trait]
impl AgentTool for ScreenshotTool {
    fn name(&self) -> &str {
        "screenshot"
    }

    fn description(&self) -> &str {
        "Take a screenshot of the current browser page. \
         Returns the screenshot as base64-encoded image data or a file path."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Optional file path to save the screenshot to. If omitted, returns base64 data.",
                    "default": "/workspace/screenshot.png"
                }
            },
            "required": []
        })
    }

    fn display_message(&self, _params: &Value) -> String {
        "Taking a screenshot".to_string()
    }

    async fn execute(
        &self,
        params: Value,
        sandbox: &SandboxHandle,
    ) -> Result<ToolResult, AppError> {
        let path = params
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("/workspace/screenshot.png");

        let result = sandbox
            .exec(&["agent-browser", "screenshot", "--path", path])
            .await?;

        if result.exit_code != 0 {
            return Ok(ToolResult {
                output: json!({
                    "error": format!("Screenshot failed: {}", result.stderr.trim()),
                    "exit_code": result.exit_code,
                }),
                artifacts: Vec::new(),
                display: format!("Failed to take screenshot: {}", result.stderr.trim()),
            });
        }

        Ok(ToolResult {
            output: json!({
                "path": path,
                "result": result.stdout,
            }),
            artifacts: vec![path.to_string()],
            display: format!("Screenshot saved to {path}"),
        })
    }
}

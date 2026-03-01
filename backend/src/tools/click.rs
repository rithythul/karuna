use async_trait::async_trait;
use serde_json::{json, Value};

use crate::error::AppError;

use super::{AgentTool, SandboxHandle, ToolResult};

/// Tool that clicks an element on the page by its ARIA ref.
pub struct ClickTool;

#[async_trait]
impl AgentTool for ClickTool {
    fn name(&self) -> &str {
        "click"
    }

    fn description(&self) -> &str {
        "Click on an element in the browser page identified by its ARIA ref \
         from the page snapshot."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "ref": {
                    "type": "string",
                    "description": "The ARIA ref of the element to click (from the page snapshot)"
                }
            },
            "required": ["ref"]
        })
    }

    fn display_message(&self, _params: &Value) -> String {
        "Clicking on element".to_string()
    }

    async fn execute(
        &self,
        params: Value,
        sandbox: &SandboxHandle,
    ) -> Result<ToolResult, AppError> {
        let element_ref = params
            .get("ref")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::BadRequest("click: missing 'ref' parameter".into()))?;

        let result = sandbox
            .exec(&["agent-browser", "click", "--ref", element_ref])
            .await?;

        if result.exit_code != 0 {
            return Ok(ToolResult {
                output: json!({
                    "error": format!("Click failed: {}", result.stderr.trim()),
                    "exit_code": result.exit_code,
                }),
                artifacts: Vec::new(),
                display: format!("Failed to click element: {}", result.stderr.trim()),
            });
        }

        Ok(ToolResult {
            output: json!({
                "ref": element_ref,
                "result": result.stdout,
            }),
            artifacts: Vec::new(),
            display: "Clicked element".to_string(),
        })
    }
}

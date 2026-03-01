use async_trait::async_trait;
use serde_json::{json, Value};

use crate::error::AppError;

use super::{AgentTool, SandboxHandle, ToolResult};

/// Tool that fills an input field on the page.
pub struct FillTool;

#[async_trait]
impl AgentTool for FillTool {
    fn name(&self) -> &str {
        "fill"
    }

    fn description(&self) -> &str {
        "Fill in a form field on the page identified by its ARIA ref. \
         Replaces the current value of the field with the provided text."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "ref": {
                    "type": "string",
                    "description": "The ARIA ref of the input field to fill (from the page snapshot)"
                },
                "value": {
                    "type": "string",
                    "description": "The text value to fill into the field"
                }
            },
            "required": ["ref", "value"]
        })
    }

    fn display_message(&self, _params: &Value) -> String {
        "Filling in form field".to_string()
    }

    async fn execute(
        &self,
        params: Value,
        sandbox: &SandboxHandle,
    ) -> Result<ToolResult, AppError> {
        let element_ref = params
            .get("ref")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::BadRequest("fill: missing 'ref' parameter".into()))?;

        let value = params
            .get("value")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::BadRequest("fill: missing 'value' parameter".into()))?;

        let result = sandbox
            .exec(&["agent-browser", "fill", "--ref", element_ref, value])
            .await?;

        if result.exit_code != 0 {
            return Ok(ToolResult {
                output: json!({
                    "error": format!("Fill failed: {}", result.stderr.trim()),
                    "exit_code": result.exit_code,
                }),
                artifacts: Vec::new(),
                display: format!("Failed to fill field: {}", result.stderr.trim()),
            });
        }

        Ok(ToolResult {
            output: json!({
                "ref": element_ref,
                "value": value,
                "result": result.stdout,
            }),
            artifacts: Vec::new(),
            display: "Filled form field".to_string(),
        })
    }
}

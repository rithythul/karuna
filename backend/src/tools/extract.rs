use async_trait::async_trait;
use serde_json::{json, Value};

use crate::error::AppError;

use super::{AgentTool, SandboxHandle, ToolResult};

/// Tool that extracts text content from an element on the page.
pub struct ExtractTool;

#[async_trait]
impl AgentTool for ExtractTool {
    fn name(&self) -> &str {
        "extract"
    }

    fn description(&self) -> &str {
        "Extract the text content of an element on the page identified by its ARIA ref."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "ref": {
                    "type": "string",
                    "description": "The ARIA ref of the element to extract text from (from the page snapshot)"
                }
            },
            "required": ["ref"]
        })
    }

    fn display_message(&self, _params: &Value) -> String {
        "Extracting content from page".to_string()
    }

    async fn execute(
        &self,
        params: Value,
        sandbox: &SandboxHandle,
    ) -> Result<ToolResult, AppError> {
        let element_ref = params
            .get("ref")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::BadRequest("extract: missing 'ref' parameter".into()))?;

        let result = sandbox
            .exec(&["agent-browser", "extract", "--ref", element_ref])
            .await?;

        if result.exit_code != 0 {
            return Ok(ToolResult {
                output: json!({
                    "error": format!("Extract failed: {}", result.stderr.trim()),
                    "exit_code": result.exit_code,
                }),
                artifacts: Vec::new(),
                display: format!("Failed to extract content: {}", result.stderr.trim()),
            });
        }

        Ok(ToolResult {
            output: json!({
                "ref": element_ref,
                "content": result.stdout,
            }),
            artifacts: Vec::new(),
            display: "Extracted page content".to_string(),
        })
    }
}

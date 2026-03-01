use async_trait::async_trait;
use serde_json::{json, Value};

use crate::error::AppError;

use super::{AgentTool, SandboxHandle, ToolResult};

/// Tool that reads a file from the sandbox container.
pub struct ReadFileTool;

#[async_trait]
impl AgentTool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read the contents of a file at the given path inside the sandbox."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute path to the file to read"
                }
            },
            "required": ["path"]
        })
    }

    fn display_message(&self, params: &Value) -> String {
        let path = params
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        format!("Reading file {path}")
    }

    async fn execute(
        &self,
        params: Value,
        sandbox: &SandboxHandle,
    ) -> Result<ToolResult, AppError> {
        let path = params
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::BadRequest("read_file: missing 'path' parameter".into()))?;

        let result = sandbox.exec(&["cat", path]).await?;

        if result.exit_code != 0 {
            return Ok(ToolResult {
                output: json!({
                    "error": format!("Failed to read file: {}", result.stderr.trim()),
                    "exit_code": result.exit_code,
                }),
                artifacts: Vec::new(),
                display: format!("Failed to read {path}: {}", result.stderr.trim()),
            });
        }

        Ok(ToolResult {
            output: json!({
                "content": result.stdout,
                "path": path,
            }),
            artifacts: Vec::new(),
            display: format!("Read {} ({} bytes)", path, result.stdout.len()),
        })
    }
}

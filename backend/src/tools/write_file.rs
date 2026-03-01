use async_trait::async_trait;
use serde_json::{json, Value};

use crate::error::AppError;

use super::{AgentTool, SandboxHandle, ToolResult};

/// Tool that writes content to a file inside the sandbox container.
pub struct WriteFileTool;

#[async_trait]
impl AgentTool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write content to a file at the given path inside the sandbox. \
         Creates parent directories automatically if they don't exist."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute path to the file to write"
                },
                "content": {
                    "type": "string",
                    "description": "The content to write to the file"
                }
            },
            "required": ["path", "content"]
        })
    }

    fn display_message(&self, params: &Value) -> String {
        let path = params
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        format!("Writing to {path}")
    }

    async fn execute(
        &self,
        params: Value,
        sandbox: &SandboxHandle,
    ) -> Result<ToolResult, AppError> {
        let path = params
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::BadRequest("write_file: missing 'path' parameter".into()))?;

        let content = params
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AppError::BadRequest("write_file: missing 'content' parameter".into())
            })?;

        sandbox.write_file(path, content).await?;

        Ok(ToolResult {
            output: json!({
                "path": path,
                "bytes_written": content.len(),
                "success": true,
            }),
            artifacts: vec![path.to_string()],
            display: format!("Wrote {} bytes to {path}", content.len()),
        })
    }
}

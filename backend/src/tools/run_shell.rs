use async_trait::async_trait;
use serde_json::{json, Value};

use crate::error::AppError;

use super::{AgentTool, SandboxHandle, ToolResult};

/// Tool that executes shell commands inside a sandbox container.
pub struct RunShellTool;

#[async_trait]
impl AgentTool for RunShellTool {
    fn name(&self) -> &str {
        "run_shell"
    }

    fn description(&self) -> &str {
        "Execute a shell command inside the sandbox environment. \
         Returns stdout, stderr, and the exit code."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "timeout": {
                    "type": "number",
                    "description": "Timeout in seconds (default: 120)",
                    "default": 120
                }
            },
            "required": ["command"]
        })
    }

    fn display_message(&self, params: &Value) -> String {
        let command = params
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        format!("Running command: `{command}`")
    }

    async fn execute(
        &self,
        params: Value,
        sandbox: &SandboxHandle,
    ) -> Result<ToolResult, AppError> {
        let command = params
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::BadRequest("run_shell: missing 'command' parameter".into()))?;

        let timeout = params
            .get("timeout")
            .and_then(|v| v.as_f64())
            .unwrap_or(120.0) as u64;

        // Wrap the command with `timeout` to enforce the time limit.
        let wrapped = format!("timeout {timeout} bash -c {}", shell_quote(command));
        let result = sandbox.exec(&["bash", "-c", &wrapped]).await?;

        let output = json!({
            "stdout": result.stdout,
            "stderr": result.stderr,
            "exit_code": result.exit_code,
        });

        let display = if result.exit_code == 0 {
            format!(
                "Command succeeded (exit 0)\n{}",
                truncate(&result.stdout, 500)
            )
        } else {
            format!(
                "Command failed (exit {})\nstderr: {}",
                result.exit_code,
                truncate(&result.stderr, 500)
            )
        };

        Ok(ToolResult {
            output,
            artifacts: Vec::new(),
            display,
        })
    }
}

/// Wrap a string in single quotes for safe embedding in a bash command.
fn shell_quote(s: &str) -> String {
    // Replace each ' with '\'' (end quote, escaped quote, start quote).
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Truncate a string to at most `max` bytes (at a valid UTF-8 char boundary).
fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        let mut end = max;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    }
}

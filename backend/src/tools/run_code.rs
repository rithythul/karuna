use async_trait::async_trait;
use serde_json::{json, Value};

use crate::error::AppError;

use super::{AgentTool, SandboxHandle, ToolResult};

/// Tool that writes code to a file and executes it in the sandbox.
pub struct RunCodeTool;

#[async_trait]
impl AgentTool for RunCodeTool {
    fn name(&self) -> &str {
        "run_code"
    }

    fn description(&self) -> &str {
        "Write code to a file and execute it in the sandbox. \
         Supports Python, JavaScript, TypeScript, Rust, Go, Ruby, Bash, C, C++, and Java."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "code": {
                    "type": "string",
                    "description": "The source code to execute"
                },
                "language": {
                    "type": "string",
                    "description": "The programming language (e.g. python, javascript, typescript, rust, go, ruby, bash, c, cpp, java)",
                    "default": "python"
                }
            },
            "required": ["code"]
        })
    }

    fn display_message(&self, params: &Value) -> String {
        let language = params
            .get("language")
            .and_then(|v| v.as_str())
            .unwrap_or("python");
        format!("Running {language} code")
    }

    async fn execute(
        &self,
        params: Value,
        sandbox: &SandboxHandle,
    ) -> Result<ToolResult, AppError> {
        let code = params
            .get("code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::BadRequest("run_code: missing 'code' parameter".into()))?;

        let language = params
            .get("language")
            .and_then(|v| v.as_str())
            .unwrap_or("python");

        let ext = extension_for(language);
        let file_path = format!("/workspace/solution.{ext}");

        // Write code to file
        sandbox.write_file(&file_path, code).await?;

        // Build and execute the run command
        let run_cmd = run_command_for(language, &file_path);
        let cmd_str: Vec<&str> = run_cmd.iter().map(|s| s.as_str()).collect();
        let result = sandbox.exec(&cmd_str).await?;

        let output = json!({
            "stdout": result.stdout,
            "stderr": result.stderr,
            "exit_code": result.exit_code,
            "language": language,
        });

        let display = if result.exit_code == 0 {
            format!(
                "{} code executed successfully\n{}",
                language,
                truncate(&result.stdout, 500)
            )
        } else {
            format!(
                "{} code failed (exit {})\n{}",
                language,
                result.exit_code,
                truncate(&result.stderr, 500)
            )
        };

        Ok(ToolResult {
            output,
            artifacts: vec![file_path],
            display,
        })
    }
}

fn extension_for(language: &str) -> String {
    let lang = language.to_lowercase();
    match lang.as_str() {
        "python" | "py" => "py".to_string(),
        "javascript" | "js" => "js".to_string(),
        "typescript" | "ts" => "ts".to_string(),
        "rust" | "rs" => "rs".to_string(),
        "go" => "go".to_string(),
        "ruby" | "rb" => "rb".to_string(),
        "bash" | "sh" | "shell" => "sh".to_string(),
        "c" => "c".to_string(),
        "cpp" | "c++" => "cpp".to_string(),
        "java" => "java".to_string(),
        _ => lang,
    }
}

fn run_command_for(language: &str, path: &str) -> Vec<String> {
    let lang = language.to_lowercase();
    match lang.as_str() {
        "python" | "py" => vec!["python3".into(), path.into()],
        "javascript" | "js" => vec!["node".into(), path.into()],
        "typescript" | "ts" => vec!["npx".into(), "ts-node".into(), path.into()],
        "rust" | "rs" => vec![
            "bash".into(),
            "-c".into(),
            format!("rustc {path} -o /tmp/solution && /tmp/solution"),
        ],
        "go" => vec!["go".into(), "run".into(), path.into()],
        "ruby" | "rb" => vec!["ruby".into(), path.into()],
        "bash" | "sh" | "shell" => vec!["bash".into(), path.into()],
        "c" => vec![
            "bash".into(),
            "-c".into(),
            format!("gcc {path} -o /tmp/solution && /tmp/solution"),
        ],
        "cpp" | "c++" => vec![
            "bash".into(),
            "-c".into(),
            format!("g++ {path} -o /tmp/solution && /tmp/solution"),
        ],
        "java" => vec![
            "bash".into(),
            "-c".into(),
            format!("cd /workspace && javac {path} && java -cp /workspace Solution"),
        ],
        _ => vec![
            "bash".into(),
            "-c".into(),
            format!("cat {path}"),
        ],
    }
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

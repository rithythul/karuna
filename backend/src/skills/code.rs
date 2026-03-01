use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::error::AppError;
use crate::llm::ChatMessage;
use crate::soul;
use super::{Skill, SkillContext, SkillOutput};

const MAX_RETRIES: u32 = 3;

pub struct CodeSkill;

impl CodeSkill {
    /// Map a language name to its file extension.
    fn extension_for(language: &str) -> String {
        let lower = language.to_lowercase();
        match lower.as_str() {
            "python" | "py" => "py",
            "javascript" | "js" => "js",
            "typescript" | "ts" => "ts",
            "rust" | "rs" => "rs",
            "go" => "go",
            "ruby" | "rb" => "rb",
            "bash" | "sh" | "shell" => "sh",
            "c" => "c",
            "cpp" | "c++" => "cpp",
            "java" => "java",
            _ => return lower,
        }
        .to_string()
    }

    /// Map a language name to the command used to run the file.
    fn run_command_for(language: &str, path: &str) -> Vec<String> {
        let lower = language.to_lowercase();
        match lower.as_str() {
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
            _ => vec!["bash".into(), "-c".into(), format!("cat {path}")],
        }
    }

    /// Ask the planning LLM to generate code for a task.
    async fn generate_code(
        ctx: &SkillContext,
        task: &str,
        language: &str,
        error_context: Option<&str>,
    ) -> Result<String, AppError> {
        let system = soul::system_prompt(
            &format!(
                "You are an expert {language} programmer. Write clean, working code \
                 that solves the given task. Return ONLY the code, no markdown fences, \
                 no explanation. The code must be complete and runnable as a single file."
            ),
            None,
        );
        let mut messages = vec![
            ChatMessage {
                role: "system".into(),
                content: system,
            },
            ChatMessage {
                role: "user".into(),
                content: format!("Task: {task}"),
            },
        ];

        if let Some(error) = error_context {
            messages.push(ChatMessage {
                role: "user".into(),
                content: format!(
                    "The previous code failed with the following error. \
                     Please fix it and return the corrected complete code only:\n\n{error}"
                ),
            });
        }

        let response = ctx.llm.plan(messages).await?;

        // Strip markdown code fences if the LLM included them
        let code = response
            .trim()
            .trim_start_matches(&format!("```{language}"))
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim()
            .to_string();

        Ok(code)
    }

    /// Write code to the sandbox container.
    async fn write_code_to_sandbox(
        ctx: &SkillContext,
        path: &str,
        code: &str,
    ) -> Result<(), AppError> {
        let escaped = code.replace('\'', "'\"'\"'");
        let cmd = format!("cat > {path} << 'KARUNA_EOF'\n{escaped}\nKARUNA_EOF");

        let result = ctx
            .sandbox
            .exec_cmd(&ctx.container_id, vec!["bash", "-c", &cmd])
            .await?;

        if result.exit_code != 0 {
            return Err(AppError::Sandbox(format!(
                "Failed to write {path}: {}",
                result.stderr
            )));
        }

        Ok(())
    }

    /// Execute code in the sandbox and return (stdout, stderr, exit_code).
    async fn execute_in_sandbox(
        ctx: &SkillContext,
        language: &str,
        path: &str,
    ) -> Result<(String, String, i64), AppError> {
        let run_cmd = Self::run_command_for(language, path);
        let cmd_refs: Vec<&str> = run_cmd.iter().map(|s| s.as_str()).collect();

        let result = ctx
            .sandbox
            .exec_cmd(&ctx.container_id, cmd_refs)
            .await?;

        Ok((result.stdout, result.stderr, result.exit_code))
    }
}

#[async_trait]
impl Skill for CodeSkill {
    fn name(&self) -> &str {
        "code"
    }
    fn description(&self) -> &str {
        "Write, execute, and test code in a sandboxed environment"
    }
    fn input_schema(&self) -> &str {
        r#"{"task": "(required) description of what code to write", "language": "(optional, default python) programming language"}"#
    }

    async fn execute(
        &self,
        ctx: &SkillContext,
        input: Value,
    ) -> Result<SkillOutput, AppError> {
        // 1. Extract task and language
        let task = input
            .get("task")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::BadRequest("Missing 'task' in input".into()))?;

        let language = input
            .get("language")
            .and_then(|v| v.as_str())
            .unwrap_or("python");

        let ext = Self::extension_for(language);
        let file_path = format!("/workspace/solution.{ext}");

        info!(
            task_id = ctx.task_id.as_str(),
            task = task,
            language = language,
            "Starting code skill"
        );

        // 2. Generate initial code
        let mut code = Self::generate_code(ctx, task, language, None).await?;
        let mut attempt = 1u32;
        let mut last_stdout;
        let mut last_stderr;

        loop {
            // 3. Write code to sandbox
            Self::write_code_to_sandbox(ctx, &file_path, &code).await?;

            // 4. Execute code
            let (stdout, stderr, exit_code) =
                Self::execute_in_sandbox(ctx, language, &file_path).await?;

            last_stdout = stdout;
            last_stderr = stderr.clone();

            if exit_code == 0 {
                info!(
                    task_id = ctx.task_id.as_str(),
                    attempt = attempt,
                    "Code executed successfully"
                );
                return Ok(SkillOutput {
                    success: true,
                    result: json!({
                        "code": code,
                        "output": last_stdout,
                        "language": language,
                        "attempts": attempt,
                        "file": file_path,
                    }),
                    artifacts: vec![file_path],
                });
            }

            // 5. Execution failed
            warn!(
                task_id = ctx.task_id.as_str(),
                attempt = attempt,
                exit_code = exit_code,
                "Code execution failed"
            );

            if attempt >= MAX_RETRIES {
                break;
            }

            // Send error back to LLM for a fix
            let error_context = format!(
                "Exit code: {exit_code}\nStdout:\n{last_stdout}\nStderr:\n{stderr}"
            );
            code = Self::generate_code(ctx, task, language, Some(&error_context)).await?;
            attempt += 1;
        }

        // All retries exhausted
        Ok(SkillOutput {
            success: false,
            result: json!({
                "code": code,
                "output": last_stdout,
                "error": last_stderr,
                "language": language,
                "attempts": attempt,
                "file": file_path,
            }),
            artifacts: vec![file_path],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_skill_metadata() {
        let skill = CodeSkill;
        assert_eq!(skill.name(), "code");
        assert!(!skill.description().is_empty());
    }

    #[test]
    fn test_extension_mapping() {
        assert_eq!(CodeSkill::extension_for("python"), "py");
        assert_eq!(CodeSkill::extension_for("Python"), "py");
        assert_eq!(CodeSkill::extension_for("javascript"), "js");
        assert_eq!(CodeSkill::extension_for("rust"), "rs");
        assert_eq!(CodeSkill::extension_for("go"), "go");
        assert_eq!(CodeSkill::extension_for("bash"), "sh");
        assert_eq!(CodeSkill::extension_for("c"), "c");
        assert_eq!(CodeSkill::extension_for("cpp"), "cpp");
        assert_eq!(CodeSkill::extension_for("java"), "java");
        assert_eq!(CodeSkill::extension_for("Unknown"), "unknown");
    }

    #[test]
    fn test_run_command_for_python() {
        let cmd = CodeSkill::run_command_for("python", "/workspace/solution.py");
        assert_eq!(cmd, vec!["python3", "/workspace/solution.py"]);
    }

    #[test]
    fn test_run_command_for_compiled_lang() {
        let cmd = CodeSkill::run_command_for("rust", "/workspace/solution.rs");
        assert_eq!(cmd.len(), 3);
        assert_eq!(cmd[0], "bash");
        assert!(cmd[2].contains("rustc"));
    }

    #[test]
    fn test_default_language() {
        let input = json!({"task": "hello world"});
        let lang = input
            .get("language")
            .and_then(|v| v.as_str())
            .unwrap_or("python");
        assert_eq!(lang, "python");
    }

    #[test]
    fn test_missing_task_detected() {
        let input = json!({"language": "python"});
        let task = input.get("task").and_then(|v| v.as_str());
        assert!(task.is_none());
    }
}

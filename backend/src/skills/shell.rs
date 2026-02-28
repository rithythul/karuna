use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::error::AppError;
use super::{Skill, SkillContext, SkillOutput};

/// Shell skill: execute arbitrary shell commands in the sandbox.
/// Useful for installing packages, running builds, system operations, etc.
pub struct ShellSkill;

#[async_trait]
impl Skill for ShellSkill {
    fn name(&self) -> &str { "shell" }
    fn description(&self) -> &str {
        "Execute shell commands in the sandbox (install packages, run builds, system operations)"
    }
    fn input_schema(&self) -> &str {
        r#"{"command": "(required) shell command to execute", "working_directory": "(optional) directory to run in, default /workspace"}"#
    }

    async fn execute(
        &self,
        ctx: &SkillContext,
        input: Value,
    ) -> Result<SkillOutput, AppError> {
        let command = input.get("command").and_then(|v| v.as_str())
            .ok_or_else(|| AppError::BadRequest("Missing 'command' in shell input".into()))?;
        let workdir = input.get("working_directory").and_then(|v| v.as_str())
            .unwrap_or("/workspace");

        info!(task_id = ctx.task_id.as_str(), command, "Executing shell command");

        let full_cmd = format!("cd {workdir} && {command}");
        let result = ctx.sandbox.exec_cmd(
            &ctx.container_id,
            vec!["bash", "-c", &format!("timeout 300 bash -c '{}'", full_cmd.replace('\'', "'\"'\"'"))],
        ).await?;

        let success = result.exit_code == 0;
        if !success {
            warn!(
                task_id = ctx.task_id.as_str(),
                exit_code = result.exit_code,
                "Shell command failed"
            );
        }

        Ok(SkillOutput {
            success,
            result: json!({
                "command": command,
                "stdout": result.stdout,
                "stderr": result.stderr,
                "exit_code": result.exit_code,
                "working_directory": workdir,
            }),
            artifacts: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_skill_metadata() {
        let skill = ShellSkill;
        assert_eq!(skill.name(), "shell");
        assert!(skill.description().contains("shell"));
    }
}

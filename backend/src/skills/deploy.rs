use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::info;

use crate::error::AppError;
use crate::llm::ChatMessage;
use crate::soul;
use super::{Skill, SkillContext, SkillOutput};

/// Deploy skill: package artifacts and deploy simple web apps or static sites.
pub struct DeploySkill;

impl DeploySkill {
    async fn generate_deploy_script(
        ctx: &SkillContext,
        task: &str,
        workspace_files: &str,
    ) -> Result<String, AppError> {
        let system = soul::system_prompt(
            "You are a deployment engineer. Generate a bash script that packages \
             and deploys the project in /workspace.\n\n\
             Options (choose the most appropriate):\n\
             1. For static sites: create a tar.gz archive of the build output\n\
             2. For Python apps: create a requirements.txt + start script\n\
             3. For Node.js apps: ensure package.json + build + start script\n\
             4. For any project: create a Dockerfile and docker-compose.yml\n\n\
             Always:\n\
             - Create a /workspace/deploy/ directory with all deployment artifacts\n\
             - Create a /workspace/deploy/README.md with deployment instructions\n\
             - Create a tar.gz archive at /workspace/deploy.tar.gz\n\
             - Print a JSON summary of what was packaged to stdout\n\
             - Return ONLY the bash script, no markdown fences",
            None,
        );
        let messages = vec![
            ChatMessage {
                role: "system".into(),
                content: system,
            },
            ChatMessage {
                role: "user".into(),
                content: format!(
                    "Task: {task}\n\nFiles in workspace:\n{workspace_files}"
                ),
            },
        ];

        let response = ctx.llm.plan(messages).await?;
        let script = response
            .trim()
            .trim_start_matches("```bash")
            .trim_start_matches("```sh")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim()
            .to_string();
        Ok(script)
    }
}

#[async_trait]
impl Skill for DeploySkill {
    fn name(&self) -> &str { "deploy" }
    fn description(&self) -> &str {
        "Package and deploy web apps, APIs, or static sites with auto-generated deployment configs"
    }
    fn input_schema(&self) -> &str {
        r#"{"task": "(required) what to deploy and how", "target": "(optional) deployment target: 'archive', 'docker', 'static'"}"#
    }

    async fn execute(
        &self,
        ctx: &SkillContext,
        input: Value,
    ) -> Result<SkillOutput, AppError> {
        let task = input.get("task").and_then(|v| v.as_str())
            .ok_or_else(|| AppError::BadRequest("Missing 'task' in deploy input".into()))?;

        info!(task_id = ctx.task_id.as_str(), task, "Starting deploy skill");

        // List workspace files for context
        let ls = ctx.sandbox.exec_cmd(
            &ctx.container_id,
            vec!["bash", "-c", "find /workspace -maxdepth 3 -not -path '*/node_modules/*' -not -path '*/.git/*' | head -100"],
        ).await?;

        let script = Self::generate_deploy_script(ctx, task, &ls.stdout).await?;

        // Write and execute deploy script
        let escaped = script.replace('\'', "'\"'\"'");
        let write_cmd = format!("cat > /workspace/deploy.sh << 'KARUNA_EOF'\n{escaped}\nKARUNA_EOF");
        ctx.sandbox.exec_cmd(&ctx.container_id, vec!["bash", "-c", &write_cmd]).await?;

        let result = ctx.sandbox.exec_cmd(
            &ctx.container_id,
            vec!["bash", "-c", "cd /workspace && timeout 180 bash deploy.sh 2>&1"],
        ).await?;

        // Check for deployment artifacts
        let artifacts_check = ctx.sandbox.exec_cmd(
            &ctx.container_id,
            vec!["bash", "-c", "find /workspace/deploy -type f 2>/dev/null | head -50; ls /workspace/deploy.tar.gz 2>/dev/null"],
        ).await?;

        let artifacts: Vec<String> = artifacts_check.stdout
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect();

        let success = result.exit_code == 0;

        Ok(SkillOutput {
            success,
            result: json!({
                "output": result.stdout,
                "error": if success { Value::Null } else { json!(result.stderr) },
                "script": script,
                "deployment_artifacts": artifacts,
            }),
            artifacts: {
                let mut a = vec!["/workspace/deploy.sh".to_string()];
                a.extend(artifacts);
                a
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deploy_skill_metadata() {
        let skill = DeploySkill;
        assert_eq!(skill.name(), "deploy");
        assert!(skill.description().contains("deploy"));
    }
}

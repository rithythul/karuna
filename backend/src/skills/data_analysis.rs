use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::error::AppError;
use crate::llm::ChatMessage;
use crate::soul;
use super::{Skill, SkillContext, SkillOutput};

/// Data analysis skill: process, analyze, and visualize data using Python.
pub struct DataAnalysisSkill;

impl DataAnalysisSkill {
    async fn generate_analysis_script(
        ctx: &SkillContext,
        task: &str,
        data_context: Option<&str>,
        error_context: Option<&str>,
    ) -> Result<String, AppError> {
        let data_hint = data_context
            .map(|d| format!("\n\nAvailable data context:\n{d}"))
            .unwrap_or_default();
        let error_hint = error_context
            .map(|e| format!("\n\nPrevious attempt failed with this error. Fix it:\n{e}"))
            .unwrap_or_default();

        let system = soul::system_prompt(
            "You are an expert data analyst and Python programmer.\n\
             Write a complete, runnable Python script that performs the requested analysis.\n\n\
             Rules:\n\
             - Use pandas for data manipulation\n\
             - Use matplotlib/seaborn for visualizations, save to /workspace/ as PNG files\n\
             - Use `plt.savefig('/workspace/chart_name.png', dpi=150, bbox_inches='tight')` — never `plt.show()`\n\
             - Print a JSON summary of key findings to stdout\n\
             - If creating sample/synthetic data, make it realistic\n\
             - Save processed data to /workspace/ as CSV\n\
             - Import all required libraries at the top\n\
             - Return ONLY the Python code, no markdown fences, no explanation",
            None,
        );
        let messages = vec![
            ChatMessage {
                role: "system".into(),
                content: system,
            },
            ChatMessage {
                role: "user".into(),
                content: format!("Task: {task}{data_hint}{error_hint}"),
            },
        ];

        let response = ctx.llm.plan(messages).await?;
        let code = response
            .trim()
            .trim_start_matches("```python")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim()
            .to_string();
        Ok(code)
    }
}

#[async_trait]
impl Skill for DataAnalysisSkill {
    fn name(&self) -> &str { "data_analysis" }
    fn description(&self) -> &str {
        "Analyze data, create visualizations, and generate statistical reports using Python (pandas, matplotlib)"
    }
    fn input_schema(&self) -> &str {
        r#"{"task": "(required) description of what data analysis to perform", "data_source": "(optional) path to data file or URL"}"#
    }

    async fn execute(
        &self,
        ctx: &SkillContext,
        input: Value,
    ) -> Result<SkillOutput, AppError> {
        let task = input.get("task").and_then(|v| v.as_str())
            .ok_or_else(|| AppError::BadRequest("Missing 'task' in data_analysis input".into()))?;
        let data_source = input.get("data_source").and_then(|v| v.as_str());

        info!(task_id = ctx.task_id.as_str(), task, "Starting data analysis");

        // Install additional Python packages if needed
        ctx.sandbox.exec_cmd(
            &ctx.container_id,
            vec!["bash", "-c", "pip3 install -q matplotlib seaborn scipy 2>/dev/null || true"],
        ).await?;

        // Check for existing data files
        let data_context = if let Some(source) = data_source {
            let check = ctx.sandbox.exec_cmd(
                &ctx.container_id,
                vec!["bash", "-c", &format!("head -20 {source} 2>/dev/null || echo 'File not found'")],
            ).await?;
            Some(format!("Data source ({source}):\n{}", check.stdout))
        } else {
            // List workspace files for context
            let ls = ctx.sandbox.exec_cmd(
                &ctx.container_id,
                vec!["bash", "-c", "ls -la /workspace/ 2>/dev/null | head -20"],
            ).await?;
            if !ls.stdout.trim().is_empty() {
                Some(format!("Files in workspace:\n{}", ls.stdout))
            } else {
                None
            }
        };

        let max_retries = 3u32;
        let mut attempt = 1u32;
        let mut script = Self::generate_analysis_script(
            ctx, task, data_context.as_deref(), None,
        ).await?;

        loop {
            // Write script
            let escaped = script.replace('\'', "'\"'\"'");
            let write_cmd = format!("cat > /workspace/analysis.py << 'KARUNA_EOF'\n{escaped}\nKARUNA_EOF");
            ctx.sandbox.exec_cmd(&ctx.container_id, vec!["bash", "-c", &write_cmd]).await?;

            // Execute
            let result = ctx.sandbox.exec_cmd(
                &ctx.container_id,
                vec!["bash", "-c", "cd /workspace && timeout 180 python3 analysis.py 2>&1"],
            ).await?;

            if result.exit_code == 0 {
                info!(task_id = ctx.task_id.as_str(), attempt, "Analysis completed");

                // Discover generated artifacts
                let files_result = ctx.sandbox.exec_cmd(
                    &ctx.container_id,
                    vec!["bash", "-c", "find /workspace -maxdepth 1 -name '*.png' -o -name '*.csv' -o -name '*.json' | sort"],
                ).await?;

                let artifacts: Vec<String> = files_result.stdout
                    .lines()
                    .filter(|l| !l.is_empty())
                    .map(|l| l.to_string())
                    .collect();

                let mut all_artifacts = vec!["/workspace/analysis.py".to_string()];
                all_artifacts.extend(artifacts.clone());

                return Ok(SkillOutput {
                    success: true,
                    result: json!({
                        "output": result.stdout,
                        "script": script,
                        "generated_files": artifacts,
                        "attempts": attempt,
                    }),
                    artifacts: all_artifacts,
                });
            }

            warn!(task_id = ctx.task_id.as_str(), attempt, "Analysis failed");

            if attempt >= max_retries {
                return Ok(SkillOutput {
                    success: false,
                    result: json!({
                        "output": result.stdout,
                        "error": result.stderr,
                        "script": script,
                        "attempts": attempt,
                    }),
                    artifacts: vec!["/workspace/analysis.py".to_string()],
                });
            }

            let error = format!("Exit code: {}\nOutput:\n{}\nStderr:\n{}", result.exit_code, result.stdout, result.stderr);
            script = Self::generate_analysis_script(
                ctx, task, data_context.as_deref(), Some(&error),
            ).await?;
            attempt += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_analysis_skill_metadata() {
        let skill = DataAnalysisSkill;
        assert_eq!(skill.name(), "data_analysis");
        assert!(skill.description().contains("data"));
    }
}

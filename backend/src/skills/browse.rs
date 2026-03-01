use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::error::AppError;
use crate::llm::ChatMessage;
use crate::soul;
use super::{Skill, SkillContext, SkillOutput};

/// Browser automation skill using Playwright in the sandbox.
/// Capabilities: navigate, screenshot, extract text, click, fill forms, scrape.
pub struct BrowseSkill;

impl BrowseSkill {
    /// Generate a Playwright Python script from a natural-language browsing task.
    async fn generate_browser_script(
        ctx: &SkillContext,
        task: &str,
        url: Option<&str>,
        error_context: Option<&str>,
    ) -> Result<String, AppError> {
        let url_hint = url.map(|u| format!("\nTarget URL: {u}")).unwrap_or_default();
        let error_hint = error_context
            .map(|e| format!("\n\nThe previous script failed with this error. Fix it:\n{e}"))
            .unwrap_or_default();

        let system = soul::system_prompt(
            &format!(
                "You are an expert browser automation engineer using Python Playwright.\n\
                 Write a complete, runnable Python script that performs the requested task.\n\n\
                 Rules:\n\
                 - Use `playwright.sync_api` (synchronous API)\n\
                 - Launch Chromium in headless mode with `chromium.launch(headless=True)`\n\
                 - Always take a screenshot at the end: `page.screenshot(path='/workspace/screenshot.png')`\n\
                 - Print extracted data to stdout as JSON when extracting information\n\
                 - Use `page.wait_for_load_state('networkidle')` after navigation\n\
                 - Handle common popups/cookie banners by dismissing them\n\
                 - Set a reasonable viewport: `browser.new_page(viewport={{'width': 1280, 'height': 720}})`\n\
                 - Return ONLY the Python code, no markdown fences, no explanation"
            ),
            None,
        );
        let messages = vec![
            ChatMessage {
                role: "system".into(),
                content: system,
            },
            ChatMessage {
                role: "user".into(),
                content: format!("Task: {task}{url_hint}{error_hint}"),
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

    async fn write_and_execute(
        ctx: &SkillContext,
        script: &str,
    ) -> Result<(String, String, i64), AppError> {
        // Write script
        let escaped = script.replace('\'', "'\"'\"'");
        let write_cmd = format!("cat > /workspace/browser_task.py << 'KARUNA_EOF'\n{escaped}\nKARUNA_EOF");
        let write_result = ctx.sandbox.exec_cmd(&ctx.container_id, vec!["bash", "-c", &write_cmd]).await?;
        if write_result.exit_code != 0 {
            return Err(AppError::Sandbox(format!("Write failed: {}", write_result.stderr)));
        }

        // Execute with timeout
        let result = ctx.sandbox.exec_cmd(
            &ctx.container_id,
            vec!["bash", "-c", "cd /workspace && timeout 120 python3 browser_task.py 2>&1"],
        ).await?;

        Ok((result.stdout, result.stderr, result.exit_code))
    }
}

#[async_trait]
impl Skill for BrowseSkill {
    fn name(&self) -> &str { "browse" }
    fn description(&self) -> &str {
        "Browse websites, take screenshots, extract data, fill forms, and interact with web pages using Playwright"
    }
    fn input_schema(&self) -> &str {
        r#"{"task": "(required) what to do in the browser", "url": "(optional) starting URL to navigate to"}"#
    }

    async fn execute(
        &self,
        ctx: &SkillContext,
        input: Value,
    ) -> Result<SkillOutput, AppError> {
        let task = input.get("task").and_then(|v| v.as_str())
            .ok_or_else(|| AppError::BadRequest("Missing 'task' in browse input".into()))?;
        let url = input.get("url").and_then(|v| v.as_str());

        info!(task_id = ctx.task_id.as_str(), task = task, "Starting browse skill");

        let max_retries = 3u32;
        let mut attempt = 1u32;
        let mut script = Self::generate_browser_script(ctx, task, url, None).await?;

        loop {
            let (stdout, stderr, exit_code) = Self::write_and_execute(ctx, &script).await?;

            if exit_code == 0 {
                info!(task_id = ctx.task_id.as_str(), attempt, "Browser task succeeded");

                // Check if a screenshot was produced
                let screenshot_check = ctx.sandbox.exec_cmd(
                    &ctx.container_id,
                    vec!["test", "-f", "/workspace/screenshot.png"],
                ).await;
                let has_screenshot = screenshot_check.map(|r| r.exit_code == 0).unwrap_or(false);

                let mut artifacts = vec!["/workspace/browser_task.py".to_string()];
                if has_screenshot {
                    artifacts.push("/workspace/screenshot.png".to_string());
                }

                return Ok(SkillOutput {
                    success: true,
                    result: json!({
                        "output": stdout,
                        "script": script,
                        "has_screenshot": has_screenshot,
                        "attempts": attempt,
                    }),
                    artifacts,
                });
            }

            warn!(task_id = ctx.task_id.as_str(), attempt, exit_code, "Browser task failed");

            if attempt >= max_retries {
                return Ok(SkillOutput {
                    success: false,
                    result: json!({
                        "output": stdout,
                        "error": stderr,
                        "script": script,
                        "attempts": attempt,
                    }),
                    artifacts: vec!["/workspace/browser_task.py".to_string()],
                });
            }

            let error_context = format!("Exit code: {exit_code}\nOutput:\n{stdout}\nStderr:\n{stderr}");
            script = Self::generate_browser_script(ctx, task, url, Some(&error_context)).await?;
            attempt += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browse_skill_metadata() {
        let skill = BrowseSkill;
        assert_eq!(skill.name(), "browse");
        assert!(skill.description().contains("Browse"));
    }
}

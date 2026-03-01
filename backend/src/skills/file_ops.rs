use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::info;

use crate::error::AppError;
use crate::llm::ChatMessage;
use crate::soul;
use super::{Skill, SkillContext, SkillOutput};

/// File operations skill: create, read, edit, list, and delete files in the sandbox.
pub struct FileSkill;

#[async_trait]
impl Skill for FileSkill {
    fn name(&self) -> &str { "file" }
    fn description(&self) -> &str {
        "Create, read, edit, list, and manage files in the sandbox workspace"
    }
    fn input_schema(&self) -> &str {
        r#"{"operation": "(required) one of: create, read, edit, list, delete, tree", "path": "(required for create/read/edit/delete) file path", "content": "(for create/edit) file content or edit instructions", "directory": "(for list/tree) directory to list, default /workspace"}"#
    }

    async fn execute(
        &self,
        ctx: &SkillContext,
        input: Value,
    ) -> Result<SkillOutput, AppError> {
        let operation = input.get("operation").and_then(|v| v.as_str())
            .ok_or_else(|| AppError::BadRequest("Missing 'operation' in file input".into()))?;

        info!(task_id = ctx.task_id.as_str(), operation, "Starting file skill");

        match operation {
            "create" => self.create_file(ctx, &input).await,
            "read" => self.read_file(ctx, &input).await,
            "edit" => self.edit_file(ctx, &input).await,
            "list" => self.list_files(ctx, &input).await,
            "tree" => self.tree(ctx, &input).await,
            "delete" => self.delete_file(ctx, &input).await,
            _ => Err(AppError::BadRequest(format!("Unknown file operation: {operation}"))),
        }
    }
}

impl FileSkill {
    async fn create_file(
        &self,
        ctx: &SkillContext,
        input: &Value,
    ) -> Result<SkillOutput, AppError> {
        let path = input.get("path").and_then(|v| v.as_str())
            .ok_or_else(|| AppError::BadRequest("Missing 'path' for create".into()))?;
        let content = input.get("content").and_then(|v| v.as_str()).unwrap_or("");

        // If content is instructions rather than literal content, use LLM to generate
        let final_content = if content.len() < 50 && !content.contains('\n') && !path.ends_with(".txt") {
            // Looks like an instruction — generate actual file content
            let system = soul::system_prompt(
                "Generate the file content requested. Return ONLY the raw file content, no markdown fences, no explanation.",
                None,
            );
            let messages = vec![
                ChatMessage {
                    role: "system".into(),
                    content: system,
                },
                ChatMessage {
                    role: "user".into(),
                    content: format!("Create file '{path}' with: {content}"),
                },
            ];
            ctx.llm.fast(messages).await?
        } else {
            content.to_string()
        };

        // Ensure parent directory exists
        let parent = std::path::Path::new(path).parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "/workspace".to_string());

        ctx.sandbox.exec_cmd(&ctx.container_id, vec!["mkdir", "-p", &parent]).await?;

        let escaped = final_content.replace('\'', "'\"'\"'");
        let cmd = format!("cat > {path} << 'KARUNA_EOF'\n{escaped}\nKARUNA_EOF");
        let result = ctx.sandbox.exec_cmd(&ctx.container_id, vec!["bash", "-c", &cmd]).await?;

        if result.exit_code != 0 {
            return Err(AppError::Sandbox(format!("Failed to create {path}: {}", result.stderr)));
        }

        Ok(SkillOutput {
            success: true,
            result: json!({
                "operation": "create",
                "path": path,
                "size": final_content.len(),
            }),
            artifacts: vec![path.to_string()],
        })
    }

    async fn read_file(
        &self,
        ctx: &SkillContext,
        input: &Value,
    ) -> Result<SkillOutput, AppError> {
        let path = input.get("path").and_then(|v| v.as_str())
            .ok_or_else(|| AppError::BadRequest("Missing 'path' for read".into()))?;

        let result = ctx.sandbox.exec_cmd(&ctx.container_id, vec!["cat", path]).await?;
        if result.exit_code != 0 {
            return Err(AppError::Sandbox(format!("Failed to read {path}: {}", result.stderr)));
        }

        Ok(SkillOutput {
            success: true,
            result: json!({
                "operation": "read",
                "path": path,
                "content": result.stdout,
                "size": result.stdout.len(),
            }),
            artifacts: vec![],
        })
    }

    async fn edit_file(
        &self,
        ctx: &SkillContext,
        input: &Value,
    ) -> Result<SkillOutput, AppError> {
        let path = input.get("path").and_then(|v| v.as_str())
            .ok_or_else(|| AppError::BadRequest("Missing 'path' for edit".into()))?;
        let instructions = input.get("content").and_then(|v| v.as_str())
            .ok_or_else(|| AppError::BadRequest("Missing 'content' (edit instructions) for edit".into()))?;

        // Read current file
        let current = ctx.sandbox.exec_cmd(&ctx.container_id, vec!["cat", path]).await?;
        let current_content = if current.exit_code == 0 { &current.stdout } else { "" };

        // Ask LLM to apply edits
        let system = soul::system_prompt(
            "You are a file editor. Apply the requested edits to the file content. \
             Return ONLY the complete updated file content, no markdown fences, no explanation.",
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
                    "Current content of '{path}':\n```\n{current_content}\n```\n\n\
                     Edit instructions: {instructions}\n\n\
                     Return the complete updated file content:"
                ),
            },
        ];

        let new_content = ctx.llm.plan(messages).await?;
        let clean = new_content
            .trim()
            .trim_start_matches("```")
            .lines()
            .skip_while(|l| l.starts_with("```"))
            .collect::<Vec<_>>()
            .join("\n")
            .trim_end_matches("```")
            .trim()
            .to_string();

        let escaped = clean.replace('\'', "'\"'\"'");
        let cmd = format!("cat > {path} << 'KARUNA_EOF'\n{escaped}\nKARUNA_EOF");
        let write_result = ctx.sandbox.exec_cmd(&ctx.container_id, vec!["bash", "-c", &cmd]).await?;

        if write_result.exit_code != 0 {
            return Err(AppError::Sandbox(format!("Failed to write {path}: {}", write_result.stderr)));
        }

        Ok(SkillOutput {
            success: true,
            result: json!({
                "operation": "edit",
                "path": path,
                "size": clean.len(),
            }),
            artifacts: vec![path.to_string()],
        })
    }

    async fn list_files(
        &self,
        ctx: &SkillContext,
        input: &Value,
    ) -> Result<SkillOutput, AppError> {
        let dir = input.get("directory").and_then(|v| v.as_str()).unwrap_or("/workspace");

        let result = ctx.sandbox.exec_cmd(
            &ctx.container_id,
            vec!["bash", "-c", &format!("ls -la {dir} 2>&1")],
        ).await?;

        Ok(SkillOutput {
            success: result.exit_code == 0,
            result: json!({
                "operation": "list",
                "directory": dir,
                "output": result.stdout,
            }),
            artifacts: vec![],
        })
    }

    async fn tree(
        &self,
        ctx: &SkillContext,
        input: &Value,
    ) -> Result<SkillOutput, AppError> {
        let dir = input.get("directory").and_then(|v| v.as_str()).unwrap_or("/workspace");

        let result = ctx.sandbox.exec_cmd(
            &ctx.container_id,
            vec!["bash", "-c", &format!(
                "find {dir} -maxdepth 4 -not -path '*/node_modules/*' -not -path '*/.git/*' | head -200"
            )],
        ).await?;

        Ok(SkillOutput {
            success: result.exit_code == 0,
            result: json!({
                "operation": "tree",
                "directory": dir,
                "output": result.stdout,
            }),
            artifacts: vec![],
        })
    }

    async fn delete_file(
        &self,
        ctx: &SkillContext,
        input: &Value,
    ) -> Result<SkillOutput, AppError> {
        let path = input.get("path").and_then(|v| v.as_str())
            .ok_or_else(|| AppError::BadRequest("Missing 'path' for delete".into()))?;

        let result = ctx.sandbox.exec_cmd(&ctx.container_id, vec!["rm", "-f", path]).await?;

        Ok(SkillOutput {
            success: result.exit_code == 0,
            result: json!({
                "operation": "delete",
                "path": path,
            }),
            artifacts: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_skill_metadata() {
        let skill = FileSkill;
        assert_eq!(skill.name(), "file");
        assert!(skill.description().contains("file"));
    }
}

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::error::AppError;
use super::{Skill, SkillContext, SkillOutput};

pub struct CodeSkill;

#[async_trait]
impl Skill for CodeSkill {
    fn name(&self) -> &str { "code" }
    fn description(&self) -> &str {
        "Write, execute, and test code in a sandboxed environment"
    }

    async fn execute(&self, _ctx: &SkillContext, _input: Value) -> Result<SkillOutput, AppError> {
        // Placeholder — will be implemented in Task 7
        Ok(SkillOutput {
            success: false,
            result: json!({"error": "not yet implemented"}),
            artifacts: vec![],
        })
    }
}

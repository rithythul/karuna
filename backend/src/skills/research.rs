use async_trait::async_trait;
use serde_json::{json, Value};

use crate::error::AppError;
use super::{Skill, SkillContext, SkillOutput};

pub struct ResearchSkill;

#[async_trait]
impl Skill for ResearchSkill {
    fn name(&self) -> &str { "research" }
    fn description(&self) -> &str {
        "Research a topic using web search and LLM synthesis"
    }

    async fn execute(&self, _ctx: &SkillContext, _input: Value) -> Result<SkillOutput, AppError> {
        // Placeholder — will be implemented in Task 6
        Ok(SkillOutput {
            success: false,
            result: json!({"error": "not yet implemented"}),
            artifacts: vec![],
        })
    }
}

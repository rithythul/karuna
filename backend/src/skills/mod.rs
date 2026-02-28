pub mod research;
pub mod code;

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::AppError;
use crate::llm::LlmClient;
use crate::sandbox::SandboxManager;

/// Context provided to every skill execution
pub struct SkillContext {
    pub llm: LlmClient,
    pub sandbox: SandboxManager,
    pub container_id: String,
    pub task_id: String,
}

/// Result of a skill execution
#[derive(Debug, serde::Serialize, Clone)]
pub struct SkillOutput {
    pub success: bool,
    pub result: Value,
    pub artifacts: Vec<String>,
}

/// Every skill implements this trait
#[async_trait]
pub trait Skill: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;

    async fn execute(
        &self,
        ctx: &SkillContext,
        input: Value,
    ) -> Result<SkillOutput, AppError>;
}

/// Registry of available skills
pub struct SkillRegistry {
    skills: HashMap<String, Arc<dyn Skill>>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self { skills: HashMap::new() }
    }

    pub fn register(&mut self, skill: Arc<dyn Skill>) {
        self.skills.insert(skill.name().to_string(), skill);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Skill>> {
        self.skills.get(name).cloned()
    }

    pub fn list(&self) -> Vec<(&str, &str)> {
        self.skills.values().map(|s| (s.name(), s.description())).collect()
    }
}

/// Build the default registry with built-in skills
pub fn default_registry() -> SkillRegistry {
    let mut registry = SkillRegistry::new();
    registry.register(Arc::new(research::ResearchSkill));
    registry.register(Arc::new(code::CodeSkill));
    registry
}

mod browser;
mod code;
mod research;
mod api;
mod data_analysis;
mod deploy;

use std::sync::Arc;
use crate::agent_runtime::AgentRegistry;

/// Build the default agent registry with all built-in agents.
pub fn default_registry() -> AgentRegistry {
    let mut registry = AgentRegistry::new();
    registry.register(Arc::new(browser::BrowserAgent));
    registry.register(Arc::new(code::CodeAgent));
    registry.register(Arc::new(research::ResearchAgent));
    registry.register(Arc::new(api::ApiAgent));
    registry.register(Arc::new(data_analysis::DataAnalysisAgent));
    registry.register(Arc::new(deploy::DeployAgent));
    registry
}

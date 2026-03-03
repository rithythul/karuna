mod browser;
mod code;
mod research;
mod api;
mod data_analysis;
mod deploy;

use std::sync::Arc;
use crate::agent_runtime::AgentRegistry;
use crate::config::Config;

/// Build the default agent registry with all built-in agents.
pub fn default_registry(config: &Config) -> AgentRegistry {
    let mut registry = AgentRegistry::new();
    registry.register(Arc::new(browser::BrowserAgent));
    registry.register(Arc::new(code::CodeAgent));
    registry.register(Arc::new(research::ResearchAgent {
        search_api_key: config.search_api_key.clone(),
        search_provider: config.search_provider.clone(),
    }));
    registry.register(Arc::new(api::ApiAgent));
    registry.register(Arc::new(data_analysis::DataAnalysisAgent));
    registry.register(Arc::new(deploy::DeployAgent));
    registry
}

use std::sync::Arc;
use async_trait::async_trait;
use crate::agent_runtime::Agent;
use crate::soul;
use crate::tools::{AgentTool, DelegateTool, ReadFileTool, RunShellTool, WebSearchTool, WriteFileTool};

pub struct ResearchAgent {
    pub search_api_key: Option<String>,
    pub search_provider: String,
}

#[async_trait]
impl Agent for ResearchAgent {
    fn name(&self) -> &str {
        "research"
    }

    fn description(&self) -> &str {
        "Search the web, read pages, and synthesize research reports"
    }

    fn system_prompt(&self) -> String {
        soul::system_prompt(
            "You are a thorough research analyst. Search the web to find accurate, \
             up-to-date information on any topic. Cross-reference multiple sources \
             and synthesize findings into clear, well-structured reports. Save \
             research outputs to files when appropriate.",
            None,
        )
    }

    fn tools(&self) -> Vec<Arc<dyn AgentTool>> {
        vec![
            Arc::new(WebSearchTool::new(
                self.search_api_key.clone(),
                self.search_provider.clone(),
            )),
            Arc::new(ReadFileTool),
            Arc::new(WriteFileTool),
            Arc::new(RunShellTool),
            Arc::new(DelegateTool),
        ]
    }

    fn max_turns(&self) -> u32 {
        25
    }
}

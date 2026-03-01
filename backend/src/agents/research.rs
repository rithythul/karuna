use std::sync::Arc;
use async_trait::async_trait;
use crate::agent_runtime::Agent;
use crate::soul;
use crate::tools::{AgentTool, ReadFileTool, RunShellTool, WebSearchTool, WriteFileTool};

pub struct ResearchAgent;

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
            Arc::new(WebSearchTool),
            Arc::new(ReadFileTool),
            Arc::new(WriteFileTool),
            Arc::new(RunShellTool),
        ]
    }

    fn max_turns(&self) -> u32 {
        25
    }
}

use std::sync::Arc;
use async_trait::async_trait;
use crate::agent_runtime::Agent;
use crate::soul;
use crate::tools::{
    AgentTool, HttpRequestTool, ReadFileTool, RunCodeTool, RunShellTool, WriteFileTool,
};

pub struct ApiAgent;

#[async_trait]
impl Agent for ApiAgent {
    fn name(&self) -> &str {
        "api"
    }

    fn description(&self) -> &str {
        "Make HTTP API calls, parse responses, and chain API workflows"
    }

    fn system_prompt(&self) -> String {
        soul::system_prompt(
            "You are an API integration specialist. Make HTTP requests to REST and \
             GraphQL APIs, parse JSON/XML responses, and chain multi-step API workflows. \
             Use run_code to transform data between calls. Handle authentication, \
             pagination, and error retries gracefully.",
            None,
        )
    }

    fn tools(&self) -> Vec<Arc<dyn AgentTool>> {
        vec![
            Arc::new(HttpRequestTool),
            Arc::new(RunCodeTool),
            Arc::new(ReadFileTool),
            Arc::new(WriteFileTool),
            Arc::new(RunShellTool),
        ]
    }
}

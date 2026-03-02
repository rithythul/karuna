use std::sync::Arc;
use async_trait::async_trait;
use crate::agent_runtime::Agent;
use crate::soul;
use crate::tools::{
    AgentTool, ClickTool, DelegateTool, ExtractTool, FillTool, NavigateTool, ReadFileTool,
    RunShellTool, ScreenshotTool, SnapshotTool, WriteFileTool,
};

pub struct BrowserAgent;

#[async_trait]
impl Agent for BrowserAgent {
    fn name(&self) -> &str {
        "browser"
    }

    fn description(&self) -> &str {
        "Browse websites, fill forms, extract data, and take screenshots"
    }

    fn system_prompt(&self) -> String {
        soul::system_prompt(
            "You are an expert web browser automation agent. You navigate websites, \
             interact with pages by clicking elements and filling forms, and extract \
             structured data from web content. Use snapshot to inspect the current page \
             state before acting. Save extracted data or screenshots to files when asked.",
            None,
        )
    }

    fn tools(&self) -> Vec<Arc<dyn AgentTool>> {
        vec![
            Arc::new(NavigateTool),
            Arc::new(SnapshotTool),
            Arc::new(ClickTool),
            Arc::new(FillTool),
            Arc::new(ExtractTool),
            Arc::new(ScreenshotTool),
            Arc::new(RunShellTool),
            Arc::new(WriteFileTool),
            Arc::new(ReadFileTool),
            Arc::new(DelegateTool),
        ]
    }
}

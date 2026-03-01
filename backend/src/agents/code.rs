use std::sync::Arc;
use async_trait::async_trait;
use crate::agent_runtime::Agent;
use crate::soul;
use crate::tools::{AgentTool, ReadFileTool, RunCodeTool, RunShellTool, WriteFileTool};

pub struct CodeAgent;

#[async_trait]
impl Agent for CodeAgent {
    fn name(&self) -> &str {
        "code"
    }

    fn description(&self) -> &str {
        "Write, execute, and debug code in multiple languages"
    }

    fn system_prompt(&self) -> String {
        soul::system_prompt(
            "You are an expert programmer. Write clean, working code. \
             Use run_code to write and execute code. Use read_file and write_file \
             for file I/O. Debug errors by reading output and fixing code. \
             When done, provide the final result as text.",
            None,
        )
    }

    fn tools(&self) -> Vec<Arc<dyn AgentTool>> {
        vec![
            Arc::new(RunCodeTool),
            Arc::new(RunShellTool),
            Arc::new(ReadFileTool),
            Arc::new(WriteFileTool),
        ]
    }
}

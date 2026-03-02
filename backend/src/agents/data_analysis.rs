use std::sync::Arc;
use async_trait::async_trait;
use crate::agent_runtime::Agent;
use crate::soul;
use crate::tools::{AgentTool, DelegateTool, ReadFileTool, RunCodeTool, RunShellTool, WriteFileTool};

pub struct DataAnalysisAgent;

#[async_trait]
impl Agent for DataAnalysisAgent {
    fn name(&self) -> &str {
        "data_analysis"
    }

    fn description(&self) -> &str {
        "Analyze data, create visualizations, and generate reports"
    }

    fn system_prompt(&self) -> String {
        soul::system_prompt(
            "You are a data analysis expert. Load, clean, and analyze datasets using \
             Python (pandas, matplotlib, seaborn). Create insightful visualizations and \
             statistical summaries. Write results and charts to files. Present findings \
             in clear, actionable language.",
            None,
        )
    }

    fn tools(&self) -> Vec<Arc<dyn AgentTool>> {
        vec![
            Arc::new(RunCodeTool),
            Arc::new(ReadFileTool),
            Arc::new(WriteFileTool),
            Arc::new(RunShellTool),
            Arc::new(DelegateTool),
        ]
    }
}

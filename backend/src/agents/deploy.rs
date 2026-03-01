use std::sync::Arc;
use async_trait::async_trait;
use crate::agent_runtime::Agent;
use crate::soul;
use crate::tools::{AgentTool, ReadFileTool, RunCodeTool, RunShellTool, WriteFileTool};

pub struct DeployAgent;

#[async_trait]
impl Agent for DeployAgent {
    fn name(&self) -> &str {
        "deploy"
    }

    fn description(&self) -> &str {
        "Package and deploy applications with deployment configs"
    }

    fn system_prompt(&self) -> String {
        soul::system_prompt(
            "You are a deployment and DevOps specialist. Build, package, and deploy \
             applications using Docker, shell scripts, and CI/CD pipelines. Generate \
             Dockerfiles, compose files, and deployment configs. Validate builds and \
             verify services are running correctly.",
            None,
        )
    }

    fn tools(&self) -> Vec<Arc<dyn AgentTool>> {
        vec![
            Arc::new(RunShellTool),
            Arc::new(ReadFileTool),
            Arc::new(WriteFileTool),
            Arc::new(RunCodeTool),
        ]
    }
}

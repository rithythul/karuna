mod click;
mod extract;
mod fill;
mod http_request;
mod navigate;
mod read_file;
mod run_code;
mod run_shell;
mod screenshot;
mod snapshot;
mod web_search;
mod write_file;

pub use click::ClickTool;
pub use extract::ExtractTool;
pub use fill::FillTool;
pub use http_request::HttpRequestTool;
pub use navigate::NavigateTool;
pub use read_file::ReadFileTool;
pub use run_code::RunCodeTool;
pub use run_shell::RunShellTool;
pub use screenshot::ScreenshotTool;
pub use snapshot::SnapshotTool;
pub use web_search::WebSearchTool;
pub use write_file::WriteFileTool;

use async_trait::async_trait;
use serde_json::Value;

use crate::error::AppError;
use crate::llm::{FunctionDefinition, ToolDefinition};
use crate::sandbox::{ExecResult, SandboxManager};

// ---------------------------------------------------------------------------
// SandboxHandle — thin wrapper so tools don't pass container_id everywhere
// ---------------------------------------------------------------------------

pub struct SandboxHandle {
    pub sandbox: SandboxManager,
    pub container_id: String,
}

impl SandboxHandle {
    pub fn new(sandbox: SandboxManager, container_id: String) -> Self {
        Self {
            sandbox,
            container_id,
        }
    }

    /// Execute a command inside the sandbox container.
    pub async fn exec(&self, cmd: &[&str]) -> Result<ExecResult, AppError> {
        self.sandbox
            .exec_cmd(&self.container_id, cmd.to_vec())
            .await
    }

    /// Read a file from the sandbox container via `cat`.
    pub async fn read_file(&self, path: &str) -> Result<String, AppError> {
        let result = self.exec(&["cat", path]).await?;
        if result.exit_code != 0 {
            return Err(AppError::Sandbox(format!(
                "Failed to read {path}: {}",
                result.stderr
            )));
        }
        Ok(result.stdout)
    }

    /// Write a file inside the sandbox container.
    ///
    /// Creates parent directories automatically and uses base64 encoding to
    /// safely transfer arbitrary content.
    pub async fn write_file(&self, path: &str, content: &str) -> Result<(), AppError> {
        use base64::Engine as _;

        // Ensure parent directory exists.
        if let Some(parent) = std::path::Path::new(path).parent() {
            let parent_str = parent.to_string_lossy();
            if !parent_str.is_empty() {
                self.exec(&["mkdir", "-p", &parent_str]).await?;
            }
        }

        // Encode content as base64 and decode inside the container to avoid
        // shell quoting issues with heredocs.
        let encoded = base64::engine::general_purpose::STANDARD.encode(content);
        let cmd = format!("echo '{encoded}' | base64 -d > {path}");
        let result = self.exec(&["bash", "-c", &cmd]).await?;

        if result.exit_code != 0 {
            return Err(AppError::Sandbox(format!(
                "Failed to write {path}: {}",
                result.stderr
            )));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ToolResult — the value returned by every tool execution
// ---------------------------------------------------------------------------

pub struct ToolResult {
    /// Structured output (JSON) from the tool.
    pub output: Value,
    /// Paths or identifiers of artifacts produced (e.g. files, screenshots).
    pub artifacts: Vec<String>,
    /// Human-readable summary for the UI.
    pub display: String,
}

// ---------------------------------------------------------------------------
// AgentTool trait — implemented by each concrete tool
// ---------------------------------------------------------------------------

#[async_trait]
pub trait AgentTool: Send + Sync {
    /// Machine-readable tool name (e.g. "run_shell").
    fn name(&self) -> &str;

    /// Short description for the LLM.
    fn description(&self) -> &str;

    /// JSON Schema describing the parameters.
    fn parameters_schema(&self) -> Value;

    /// Human-readable message shown in the UI when the tool is invoked.
    fn display_message(&self, params: &Value) -> String;

    /// Execute the tool inside the given sandbox.
    async fn execute(
        &self,
        params: Value,
        sandbox: &SandboxHandle,
    ) -> Result<ToolResult, AppError>;

    /// Build the [`ToolDefinition`] sent to the LLM.
    fn to_tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: self.name().to_string(),
                description: self.description().to_string(),
                parameters: self.parameters_schema(),
            },
        }
    }
}

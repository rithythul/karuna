use async_trait::async_trait;
use serde_json::{json, Value};

use crate::error::AppError;

use super::{AgentTool, SandboxHandle, ToolResult};

/// Tool that performs web searches via DuckDuckGo lite.
pub struct WebSearchTool;

#[async_trait]
impl AgentTool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web using DuckDuckGo. Returns text search results."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                }
            },
            "required": ["query"]
        })
    }

    fn display_message(&self, params: &Value) -> String {
        let query = params
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        format!("Searching for {query}")
    }

    async fn execute(
        &self,
        params: Value,
        sandbox: &SandboxHandle,
    ) -> Result<ToolResult, AppError> {
        let query = params
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::BadRequest("web_search: missing 'query' parameter".into()))?;

        // URL-encode the query for use in the curl command
        let encoded = urlencod(query);

        let cmd = format!(
            "curl -sL --max-time 15 -A 'Mozilla/5.0' \
             'https://lite.duckduckgo.com/lite/?q={encoded}' \
             | sed -n 's/<[^>]*>//gp' | head -200"
        );

        let result = sandbox.exec(&["bash", "-c", &cmd]).await?;

        if result.exit_code != 0 {
            return Ok(ToolResult {
                output: json!({
                    "error": format!("Search failed: {}", result.stderr.trim()),
                    "exit_code": result.exit_code,
                }),
                artifacts: Vec::new(),
                display: format!("Search failed: {}", result.stderr.trim()),
            });
        }

        Ok(ToolResult {
            output: json!({
                "query": query,
                "results": result.stdout,
            }),
            artifacts: Vec::new(),
            display: format!("Search results for \"{query}\""),
        })
    }
}

/// Simple percent-encoding for URL query parameters.
fn urlencod(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push_str(&format!("{b:02X}"));
            }
        }
    }
    out
}

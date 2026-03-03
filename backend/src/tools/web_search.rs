use async_trait::async_trait;
use serde_json::{json, Value};

use crate::error::AppError;

use super::{AgentTool, SandboxHandle, ToolResult};

/// Tool that performs web searches via Brave, Serper, or DuckDuckGo lite.
pub struct WebSearchTool {
    api_key: Option<String>,
    provider: String,
    client: reqwest::Client,
}

impl WebSearchTool {
    pub fn new(api_key: Option<String>, provider: String) -> Self {
        Self {
            api_key,
            provider,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl AgentTool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        match self.provider.as_str() {
            "brave" => "Search the web using Brave Search. Returns structured results with title, URL, and description.",
            "serper" => "Search the web using Google (via Serper). Returns structured results with title, URL, and description.",
            _ => "Search the web using DuckDuckGo. Returns text search results.",
        }
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

        match (&self.api_key, self.provider.as_str()) {
            (Some(key), "brave") => search_brave(query, key, &self.client).await,
            (Some(key), "serper") => search_serper(query, key, &self.client).await,
            _ => search_duckduckgo_fallback(query, sandbox).await,
        }
    }
}

/// DuckDuckGo fallback — no API key needed, uses curl in sandbox.
async fn search_duckduckgo_fallback(
    query: &str,
    sandbox: &SandboxHandle,
) -> Result<ToolResult, AppError> {
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

/// Brave Search API — structured results, requires API key.
/// Get a free key at https://brave.com/search/api/
async fn search_brave(query: &str, api_key: &str, client: &reqwest::Client) -> Result<ToolResult, AppError> {
    let encoded = urlencod(query);
    let url = format!(
        "https://api.search.brave.com/res/v1/web/search\
         ?q={encoded}&count=10&text_decorations=false"
    );

    let resp = client
        .get(&url)
        .header("Accept", "application/json")
        .header("Accept-Encoding", "gzip")
        .header("X-Subscription-Token", api_key)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Brave search failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Ok(ToolResult {
            output: json!({ "error": format!("Brave API {status}: {body}") }),
            artifacts: Vec::new(),
            display: format!("Search failed: HTTP {status}"),
        });
    }

    let data: Value = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to parse Brave response: {e}")))?;

    let empty = vec![];
    let raw = data["web"]["results"].as_array().unwrap_or(&empty);
    let results: Vec<Value> = raw
        .iter()
        .map(|r| json!({
            "title": r["title"].as_str().unwrap_or(""),
            "url": r["url"].as_str().unwrap_or(""),
            "description": r["description"].as_str().unwrap_or(""),
        }))
        .collect();

    Ok(ToolResult {
        output: json!({ "query": query, "results": results }),
        artifacts: Vec::new(),
        display: format!("Found {} results for \"{query}\"", results.len()),
    })
}

/// Serper API (Google Search) — structured results, requires API key.
/// Get a free key at https://serper.dev/
async fn search_serper(query: &str, api_key: &str, client: &reqwest::Client) -> Result<ToolResult, AppError> {
    let resp = client
        .post("https://google.serper.dev/search")
        .header("X-API-KEY", api_key)
        .header("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(15))
        .json(&json!({ "q": query, "num": 10 }))
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Serper search failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Ok(ToolResult {
            output: json!({ "error": format!("Serper API {status}: {body}") }),
            artifacts: Vec::new(),
            display: format!("Search failed: HTTP {status}"),
        });
    }

    let data: Value = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to parse Serper response: {e}")))?;

    let empty = vec![];
    let raw = data["organic"].as_array().unwrap_or(&empty);
    let results: Vec<Value> = raw
        .iter()
        .map(|r| json!({
            "title": r["title"].as_str().unwrap_or(""),
            "url": r["link"].as_str().unwrap_or(""),
            "description": r["snippet"].as_str().unwrap_or(""),
        }))
        .collect();

    Ok(ToolResult {
        output: json!({ "query": query, "results": results }),
        artifacts: Vec::new(),
        display: format!("Found {} results for \"{query}\"", results.len()),
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_urlencod_spaces() {
        assert_eq!(urlencod("hello world"), "hello+world");
    }

    #[test]
    fn test_urlencod_special_chars() {
        assert_eq!(urlencod("a&b=c"), "a%26b%3Dc");
    }

    #[test]
    fn test_urlencod_alphanumeric_passthrough() {
        assert_eq!(urlencod("abc123"), "abc123");
    }

    #[test]
    fn websearchtool_name() {
        let tool = WebSearchTool::new(None, "brave".into());
        assert_eq!(tool.name(), "web_search");
    }

    #[test]
    fn websearchtool_display_message() {
        let tool = WebSearchTool::new(None, "brave".into());
        let params = serde_json::json!({ "query": "rust async" });
        assert_eq!(tool.display_message(&params), "Searching for rust async");
    }
}

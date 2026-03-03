# Search Quality Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace fragile DuckDuckGo-lite scraping with the Brave Search API (structured JSON results) while keeping the existing curl fallback when no API key is configured.

**Architecture:** `WebSearchTool` becomes a struct holding `api_key: Option<String>` and `provider: String`. Agent registries are constructed with config. The tool dispatches to Brave API, Serper API, or the existing DuckDuckGo curl fallback depending on config. Config adds two new optional env vars.

**Tech Stack:** Rust (reqwest already in Cargo.toml for HTTP), Brave Search API (`https://api.search.brave.com/res/v1/web/search`), Serper API (`https://google.serper.dev/search`)

---

### Task 1: Add search config fields

**Files:**
- Modify: `backend/src/config.rs`
- Modify: `.env.example`

**Step 1: Add fields to Config struct**

In `config.rs`, add to the `Config` struct after `fast_model`:

```rust
pub search_api_key: Option<String>,
pub search_provider: String,
```

**Step 2: Populate in `from_env()`**

Inside `Config::from_env()`, add after the existing model fields:

```rust
search_api_key: env::var("HANUMAN_SEARCH_API_KEY").ok(),
search_provider: env::var("HANUMAN_SEARCH_PROVIDER")
    .unwrap_or_else(|_| "brave".into()),
```

**Step 3: Update .env.example**

Add to `.env.example`:

```
# Web search (optional — falls back to DuckDuckGo if not set)
# Provider: "brave" or "serper"
# HANUMAN_SEARCH_API_KEY=your-brave-or-serper-api-key
# HANUMAN_SEARCH_PROVIDER=brave
```

**Step 4: Verify it compiles**

```bash
SQLX_OFFLINE=true cargo check 2>&1 | grep -E "^error"
```

Expected: no errors.

**Step 5: Commit**

```bash
git add backend/src/config.rs .env.example
git commit -m "feat: add search_api_key and search_provider config fields"
```

---

### Task 2: Refactor WebSearchTool to hold config and support multiple providers

**Files:**
- Modify: `backend/src/tools/web_search.rs`

**Step 1: Replace zero-size struct with config-holding struct**

At the top of the file, replace `pub struct WebSearchTool;` with:

```rust
pub struct WebSearchTool {
    api_key: Option<String>,
    provider: String,
}

impl WebSearchTool {
    pub fn new(api_key: Option<String>, provider: String) -> Self {
        Self { api_key, provider }
    }
}
```

**Step 2: Update the execute method to dispatch by provider**

Replace the body of `async fn execute(...)` with:

```rust
let query = params
    .get("query")
    .and_then(|v| v.as_str())
    .ok_or_else(|| AppError::BadRequest("web_search: missing 'query' parameter".into()))?;

match (&self.api_key, self.provider.as_str()) {
    (Some(key), "brave") => search_brave(query, key).await,
    (Some(key), "serper") => search_serper(query, key).await,
    _ => search_duckduckgo_fallback(query, sandbox).await,
}
```

**Step 3: Add the Brave Search function**

After the `AgentTool` impl block, add:

```rust
/// Brave Search API — structured results, requires API key.
/// Get a free key at https://brave.com/search/api/
async fn search_brave(query: &str, api_key: &str) -> Result<ToolResult, AppError> {
    let encoded = urlencod(query);
    let url = format!(
        "https://api.search.brave.com/res/v1/web/search\
         ?q={encoded}&count=10&text_decorations=false"
    );

    let client = reqwest::Client::new();
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
```

**Step 4: Add the Serper Search function**

```rust
/// Serper API (Google Search) — structured results, requires API key.
/// Get a free key at https://serper.dev/
async fn search_serper(query: &str, api_key: &str) -> Result<ToolResult, AppError> {
    let client = reqwest::Client::new();
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
```

**Step 5: Rename existing DuckDuckGo logic to `search_duckduckgo_fallback`**

The existing `execute` body currently does the curl-via-sandbox approach. Extract it into:

```rust
/// DuckDuckGo fallback — no API key needed, uses curl in sandbox.
async fn search_duckduckgo_fallback(
    query: &str,
    sandbox: &SandboxHandle,
) -> Result<ToolResult, AppError> {
    // ... move existing execute body here verbatim ...
}
```

**Step 6: Add unit tests**

At the bottom of the file, add:

```rust
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
```

**Step 7: Run unit tests**

```bash
SQLX_OFFLINE=true cargo test web_search 2>&1
```

Expected: 5 tests pass.

**Step 8: Commit**

```bash
git add backend/src/tools/web_search.rs
git commit -m "feat: WebSearchTool supports Brave/Serper APIs with DuckDuckGo fallback"
```

---

### Task 3: Thread config into agent registry

**Files:**
- Modify: `backend/src/agents/research.rs`
- Modify: `backend/src/agents/mod.rs`
- Modify: `backend/src/main.rs`

**Step 1: Read research.rs**

Read `backend/src/agents/research.rs` to see the current struct and tool list.

**Step 2: Update ResearchAgent struct**

Change from a zero-size struct to one holding search config:

```rust
pub struct ResearchAgent {
    pub search_api_key: Option<String>,
    pub search_provider: String,
}
```

In its `tools()` method, change `Arc::new(WebSearchTool)` to:
```rust
Arc::new(WebSearchTool::new(
    self.search_api_key.clone(),
    self.search_provider.clone(),
))
```

Make sure to also add the import at the top of research.rs if not already there:
```rust
use crate::tools::WebSearchTool;
```

**Step 3: Update agents/mod.rs**

Add config import and change signature:

```rust
use crate::config::Config;

pub fn default_registry(config: &Config) -> AgentRegistry {
    let mut registry = AgentRegistry::new();
    registry.register(Arc::new(browser::BrowserAgent));
    registry.register(Arc::new(code::CodeAgent));
    registry.register(Arc::new(research::ResearchAgent {
        search_api_key: config.search_api_key.clone(),
        search_provider: config.search_provider.clone(),
    }));
    registry.register(Arc::new(api::ApiAgent));
    registry.register(Arc::new(data_analysis::DataAnalysisAgent));
    registry.register(Arc::new(deploy::DeployAgent));
    registry
}
```

**Step 4: Update main.rs**

Change:
```rust
let agents = Arc::new(agents::default_registry());
```
To:
```rust
let agents = Arc::new(agents::default_registry(&config));
```

**Step 5: Compile and test**

```bash
SQLX_OFFLINE=true cargo check 2>&1 | grep -E "^error"
SQLX_OFFLINE=true cargo test 2>&1 | tail -20
```

Expected: no errors, all tests pass.

**Step 6: Commit**

```bash
git add backend/src/agents/research.rs backend/src/agents/mod.rs backend/src/main.rs
git commit -m "feat: thread search config into agent registry"
```

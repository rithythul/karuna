use async_trait::async_trait;
use serde_json::{json, Value};

use crate::error::AppError;

use super::{AgentTool, SandboxHandle, ToolResult};

/// Tool that makes HTTP requests via curl in the sandbox.
pub struct HttpRequestTool;

#[async_trait]
impl AgentTool for HttpRequestTool {
    fn name(&self) -> &str {
        "http_request"
    }

    fn description(&self) -> &str {
        "Make an HTTP request using curl. Supports GET, POST, PUT, and DELETE methods \
         with optional headers and request body. Returns the response body and status code."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to send the request to"
                },
                "method": {
                    "type": "string",
                    "description": "HTTP method (GET, POST, PUT, DELETE)",
                    "default": "GET",
                    "enum": ["GET", "POST", "PUT", "DELETE"]
                },
                "headers": {
                    "type": "object",
                    "description": "Optional HTTP headers as key-value pairs",
                    "additionalProperties": { "type": "string" }
                },
                "body": {
                    "type": "string",
                    "description": "Optional request body (for POST/PUT)"
                }
            },
            "required": ["url"]
        })
    }

    fn display_message(&self, params: &Value) -> String {
        let method = params
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET");
        let url = params
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        format!("Making {method} request to {url}")
    }

    async fn execute(
        &self,
        params: Value,
        sandbox: &SandboxHandle,
    ) -> Result<ToolResult, AppError> {
        let url = params
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AppError::BadRequest("http_request: missing 'url' parameter".into())
            })?;

        let method = params
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET");

        // Build curl command
        // -s: silent, -w '\n%{http_code}': append status code on last line
        let mut cmd = format!("curl -s -w '\\n%{{http_code}}' -X {method}");

        // Add headers
        if let Some(headers) = params.get("headers").and_then(|v| v.as_object()) {
            for (key, value) in headers {
                if let Some(val) = value.as_str() {
                    // Escape single quotes in header values
                    let escaped_key = key.replace('\'', "'\\''");
                    let escaped_val = val.replace('\'', "'\\''");
                    cmd.push_str(&format!(" -H '{escaped_key}: {escaped_val}'"));
                }
            }
        }

        // Add body
        if let Some(body) = params.get("body").and_then(|v| v.as_str()) {
            let escaped_body = body.replace('\'', "'\\''");
            cmd.push_str(&format!(" -d '{escaped_body}'"));
        }

        // Add URL (escape single quotes)
        let escaped_url = url.replace('\'', "'\\''");
        cmd.push_str(&format!(" '{escaped_url}'"));

        let result = sandbox.exec(&["bash", "-c", &cmd]).await?;

        if result.exit_code != 0 {
            return Ok(ToolResult {
                output: json!({
                    "error": format!("HTTP request failed: {}", result.stderr.trim()),
                    "exit_code": result.exit_code,
                }),
                artifacts: Vec::new(),
                display: format!("HTTP request failed: {}", result.stderr.trim()),
            });
        }

        // Parse status code from last line of stdout
        let stdout = result.stdout.trim_end();
        let (body_text, status_code) = match stdout.rsplit_once('\n') {
            Some((body, code)) => (body.to_string(), code.trim().to_string()),
            None => (String::new(), stdout.to_string()),
        };

        let status: i64 = status_code.parse().unwrap_or(0);

        Ok(ToolResult {
            output: json!({
                "status_code": status,
                "body": body_text,
                "method": method,
                "url": url,
            }),
            artifacts: Vec::new(),
            display: format!("{method} {url} -> {status}"),
        })
    }
}

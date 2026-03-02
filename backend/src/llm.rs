use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::error::AppError;

#[derive(Clone)]
pub struct LlmClient {
    client: Client,
    base_url: String,
    api_key: String,
    pub default_model: String,
    pub planning_model: String,
    pub fast_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
}

#[derive(Debug, Deserialize)]
struct ChatMessageResponse {
    content: String,
}

// ---------------------------------------------------------------------------
// Tool-calling types (OpenAI-compatible)
// ---------------------------------------------------------------------------

/// Defines a tool that the model may call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Always "function" for now.
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionDefinition,
}

/// The function metadata sent in a tool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    pub name: String,
    pub description: String,
    /// JSON Schema describing the parameters.
    pub parameters: serde_json::Value,
}

/// A tool call returned by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

/// The function invocation inside a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    /// JSON-encoded arguments string.
    pub arguments: String,
}

/// Unified response from an LLM call that may include tool calls.
#[derive(Debug, Clone)]
pub enum LlmResponse {
    /// The model returned plain text content.
    Text(String),
    /// The model requested one or more tool calls.
    ToolCalls(Vec<ToolCall>),
}

// ---------------------------------------------------------------------------
// Request / response types for tool-calling chat
// ---------------------------------------------------------------------------

/// Chat request that supports tool definitions and flexible message shapes.
///
/// Messages are `serde_json::Value` so callers can include tool-result
/// messages (`role: "tool"`, `tool_call_id`, …) which don't fit the
/// fixed `ChatMessage` struct.
#[derive(Debug, Serialize)]
struct ToolChatRequest {
    model: String,
    messages: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

/// Response wrapper for tool-calling chat completions.
#[derive(Debug, Deserialize)]
struct ToolChatResponse {
    choices: Vec<ToolChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ToolChatChoice {
    message: ToolChatMessageResponse,
}

/// The message part of a tool-calling response.
///
/// `content` may be null when the model responds with tool calls only.
#[derive(Debug, Deserialize)]
struct ToolChatMessageResponse {
    content: Option<String>,
    tool_calls: Option<Vec<ToolCall>>,
}

// ---------------------------------------------------------------------------
// LlmClient implementation
// ---------------------------------------------------------------------------

impl LlmClient {
    pub fn new(config: &Config) -> Self {
        Self {
            client: Client::new(),
            base_url: config.openrouter_base_url.clone(),
            api_key: config.openrouter_api_key.clone(),
            default_model: config.default_model.clone(),
            planning_model: config.planning_model.clone(),
            fast_model: config.fast_model.clone(),
        }
    }

    pub async fn chat(
        &self,
        model: &str,
        messages: Vec<ChatMessage>,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
    ) -> Result<String, AppError> {
        let request = ChatRequest {
            model: model.to_string(),
            messages,
            temperature,
            max_tokens,
        };

        let response = self.client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("HTTP-Referer", "https://hanuman.ai")
            .header("X-Title", "Hanuman AI Partner")
            .json(&request)
            .send()
            .await
            .map_err(|e| AppError::Llm(format!("Request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::Llm(format!("OpenRouter {status}: {body}")));
        }

        let chat_response: ChatResponse = response
            .json()
            .await
            .map_err(|e| AppError::Llm(format!("Failed to parse response: {e}")))?;

        chat_response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .ok_or_else(|| AppError::Llm("No response from model".into()))
    }

    /// Send a chat completion request with optional tool definitions.
    ///
    /// Messages are `serde_json::Value` to support the full range of message
    /// shapes required by the tool-calling protocol (user, assistant,
    /// tool-result, etc.).
    ///
    /// Returns [`LlmResponse::ToolCalls`] when the model wants to invoke
    /// tools, or [`LlmResponse::Text`] for a regular text reply.
    pub async fn chat_with_tools(
        &self,
        model: &str,
        messages: Vec<serde_json::Value>,
        tools: Option<Vec<ToolDefinition>>,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
    ) -> Result<LlmResponse, AppError> {
        let request = ToolChatRequest {
            model: model.to_string(),
            messages,
            tools,
            temperature,
            max_tokens,
        };

        let response = self.client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("HTTP-Referer", "https://hanuman.ai")
            .header("X-Title", "Hanuman AI Partner")
            .json(&request)
            .send()
            .await
            .map_err(|e| AppError::Llm(format!("Request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::Llm(format!("OpenRouter {status}: {body}")));
        }

        let chat_response: ToolChatResponse = response
            .json()
            .await
            .map_err(|e| AppError::Llm(format!("Failed to parse response: {e}")))?;

        let choice = chat_response
            .choices
            .first()
            .ok_or_else(|| AppError::Llm("No response from model".into()))?;

        // If the model returned tool calls, prefer those over text content.
        if let Some(ref tool_calls) = choice.message.tool_calls {
            if !tool_calls.is_empty() {
                return Ok(LlmResponse::ToolCalls(tool_calls.clone()));
            }
        }

        // Otherwise return the text content (defaulting to empty string if null).
        let text = choice.message.content.clone().unwrap_or_default();
        Ok(LlmResponse::Text(text))
    }

    pub async fn plan(&self, messages: Vec<ChatMessage>) -> Result<String, AppError> {
        let model = self.planning_model.clone();
        self.chat(&model, messages, Some(0.3), Some(4096)).await
    }

    pub async fn fast(&self, messages: Vec<ChatMessage>) -> Result<String, AppError> {
        let model = self.fast_model.clone();
        self.chat(&model, messages, Some(0.2), Some(2048)).await
    }
}

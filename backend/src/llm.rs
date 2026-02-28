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
            .header("HTTP-Referer", "https://karuna.ai")
            .header("X-Title", "Karuna AI Partner")
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

    pub async fn plan(&self, messages: Vec<ChatMessage>) -> Result<String, AppError> {
        let model = self.planning_model.clone();
        self.chat(&model, messages, Some(0.3), Some(4096)).await
    }

    pub async fn fast(&self, messages: Vec<ChatMessage>) -> Result<String, AppError> {
        let model = self.fast_model.clone();
        self.chat(&model, messages, Some(0.2), Some(2048)).await
    }
}

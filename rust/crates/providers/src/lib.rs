//! pie_providers
//!
//! Provider transport + normalization ONLY.
//! No policy. No redaction. No audit. No retries.
//! Input MUST be SanitizedModelRequest.

use async_trait::async_trait;
use pie_redaction::{PromptMessage, SanitizedModelRequest};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("invalid response: {0}")]
    InvalidResponse(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMsg {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderReply {
    pub content: String,
    pub finish_reason: Option<String>,
    pub usage: Usage,
    /// Raw provider request id if present (Rust control plane will hash it for audit)
    pub provider_request_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProviderResponse {
    pub raw_json: Value,
    pub normalized: ProviderReply,
}

#[async_trait]
pub trait Provider: Send + Sync {
    async fn dispatch(&self, req: &SanitizedModelRequest) -> Result<ProviderResponse, ProviderError>;
}

fn to_chat_msgs(messages: &[PromptMessage]) -> Vec<ChatMsg> {
    messages
        .iter()
        .map(|m| ChatMsg {
            role: m.role.clone(),
            content: m.content.clone(),
        })
        .collect()
}

pub struct OpenAICompatProvider {
    client: Client,
    base_url: String,
    api_key: Option<String>,
}

impl OpenAICompatProvider {
    pub fn new(base_url: String, api_key: Option<String>) -> Self {
        Self { client: Client::new(), base_url, api_key }
    }
}

#[derive(Debug, Serialize)]
struct OpenAICompatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMsg>,
    max_tokens: u64,
    temperature: f64,
    top_p: f64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stop: Vec<String>,
}

#[async_trait]
impl Provider for OpenAICompatProvider {
    async fn dispatch(&self, req: &SanitizedModelRequest) -> Result<ProviderResponse, ProviderError> {
        let url = format!("{}/v1/chat/completions", self.base_url.trim_end_matches('/'));
        let body = OpenAICompatRequest {
            model: &req.model.0,
            messages: to_chat_msgs(&req.prompt.messages),
            max_tokens: req.prompt.max_output_tokens,
            temperature: req.prompt.temperature,
            top_p: req.prompt.top_p,
            stop: req.prompt.stop.clone(),
        };

        let mut r = self.client.post(url).json(&body);
        if let Some(k) = &self.api_key {
            if !k.is_empty() {
                r = r.bearer_auth(k);
            }
        }
        let resp = r.send().await?;
        let raw: Value = resp.json().await?;

        // Normalize minimal shape: choices[0].message.content, finish_reason, usage
        let content = raw
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c0| c0.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| ProviderError::InvalidResponse("missing choices[0].message.content".into()))?
            .to_string();

        let finish_reason = raw
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c0| c0.get("finish_reason"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let input_tokens = raw.get("usage").and_then(|u| u.get("prompt_tokens")).and_then(|v| v.as_u64());
        let output_tokens = raw.get("usage").and_then(|u| u.get("completion_tokens")).and_then(|v| v.as_u64());

        let provider_request_id = raw.get("id").and_then(|v| v.as_str()).map(|s| s.to_string());

        Ok(ProviderResponse {
            raw_json: raw.clone(),
            normalized: ProviderReply {
                content,
                finish_reason,
                usage: Usage { input_tokens, output_tokens },
                provider_request_id,
            },
        })
    }
}

// Placeholder: Anthropic/XAI can be added as separate providers later
// You can still route "anthropic" and "xai" through OpenAICompat if your infra supports it

use crate::payload::{AddMemoryRequest, AddMemoryResponse};
use reqwest::Client;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue, CONTENT_TYPE};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OpenMemoryError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("invalid response: {0}")]
    InvalidResponse(String),
}

pub struct OpenMemoryClient {
    base_url: String,
    api_key: Option<String>,
    client: Client,
}

impl OpenMemoryClient {
    pub fn new(base_url: String, api_key: Option<String>, timeout_ms: u64) -> Result<Self, OpenMemoryError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_millis(timeout_ms))
            .build()?;
        Ok(Self { base_url, api_key, client })
    }

        pub async fn add_memory(&self, req: &AddMemoryRequest) -> Result<AddMemoryResponse, OpenMemoryError> {
        let url = format!("{}/memory/add", self.base_url.trim_end_matches('/'));

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));


        // OpenMemory deployments vary. The proven local-agent-core approach is to send BOTH when a key exists.
        if let Some(k) = &self.api_key {
            // Authorization: Bearer <key>
            let v = format!("Bearer {}", k);
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&v).map_err(|e| OpenMemoryError::InvalidResponse(e.to_string()))?,
            );

            // x-api-key: <key>
            headers.insert(
                "x-api-key",
                HeaderValue::from_str(k).map_err(|e| OpenMemoryError::InvalidResponse(e.to_string()))?,
            );

            // Keep auth_mode for future explicitness, but we don't gate header emission on it.
            // If you want strict mode later, we can reintroduce it as an opt-in.
        }

        let resp = self.client.post(url).headers(headers).json(req).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(OpenMemoryError::InvalidResponse(format!("status={} body={}", status, body)));
        }

        Ok(resp.json::<AddMemoryResponse>().await?)
    }
}
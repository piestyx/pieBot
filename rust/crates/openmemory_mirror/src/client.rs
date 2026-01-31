use crate::payload::{AddMemoryRequest, AddMemoryResponse, QueryMemoryParsed, QueryMemoryRequest, QueryHitRef};
use reqwest::Client;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue, CONTENT_TYPE};
use thiserror::Error;
use serde_json::Value as JsonValue;

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

    fn build_headers(&self) -> Result<HeaderMap, OpenMemoryError> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        // Match proven Python behavior: send BOTH auth headers if key exists.
        if let Some(k) = &self.api_key {
            let v = format!("Bearer {}", k);
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&v).map_err(|e| OpenMemoryError::InvalidResponse(e.to_string()))?,
            );
            headers.insert(
                "x-api-key",
                HeaderValue::from_str(k).map_err(|e| OpenMemoryError::InvalidResponse(e.to_string()))?,
            );
        }
        Ok(headers)
    }

        pub async fn add_memory(&self, req: &AddMemoryRequest) -> Result<AddMemoryResponse, OpenMemoryError> {
        let url = format!("{}/memory/add", self.base_url.trim_end_matches('/'));

        let headers = self.build_headers()?;

        let resp = self.client.post(url).headers(headers).json(req).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(OpenMemoryError::InvalidResponse(format!("status={} body={}", status, body)));
        }

        Ok(resp.json::<AddMemoryResponse>().await?)
    }

    pub async fn query_memory(&self, req: &QueryMemoryRequest) -> Result<QueryMemoryParsed, OpenMemoryError> {
        let url = format!("{}/memory/query", self.base_url.trim_end_matches('/'));
        let headers = self.build_headers()?;

        let resp = self.client.post(url).headers(headers).json(req).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(OpenMemoryError::InvalidResponse(format!("status={} body={}", status, body)));
        }

        let raw: JsonValue = resp.json().await?;
        let hits = extract_hit_refs(&raw);
        Ok(QueryMemoryParsed { raw, hits })
    }
}

fn extract_hit_refs(raw: &JsonValue) -> Vec<QueryHitRef> {
    // Tolerant parsing: OpenMemory responses vary. We scan common shapes:
    // - list of objects
    // - { matches/memories/results/items/data: [..] }
    // Each object may have {id|memory_id}, {content|text}, {score|salience}
    fn as_list(v: &JsonValue) -> Option<&Vec<JsonValue>> {
        v.as_array()
    }
    let mut items: Vec<&JsonValue> = vec![];
    if let Some(arr) = as_list(raw) {
        items = arr.iter().collect();
    } else if let Some(obj) = raw.as_object() {
        for k in ["matches", "memories", "results", "items", "data"] {
            if let Some(v) = obj.get(k).and_then(|x| x.as_array()) {
                items = v.iter().collect();
                break;
            }
        }
        if items.is_empty() {
            // single object fallback
            items.push(raw);
        }
    } else {
        return vec![];
    }

    let mut out = vec![];
    for it in items {
        let o = match it.as_object() {
            Some(x) => x,
            None => continue,
        };
        let id = o.get("id")
            .or_else(|| o.get("memory_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if id.is_empty() { continue; }

        let content = o.get("content")
            .or_else(|| o.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let score = o.get("score")
            .or_else(|| o.get("salience"))
            .and_then(|v| v.as_f64());

        // IMPORTANT: never return content, only hash it.
        let content_hash = pie_common::sha256_bytes(content.as_bytes());
        out.push(QueryHitRef { id, score, content_hash });
    }
    out
}
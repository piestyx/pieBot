use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// OpenMemory Backend API:
// POST /memory/add
// { content, tags?, metadata?, user_id? }

#[derive(Debug, Clone, Serialize)]
pub struct AddMemoryRequest {
    pub content: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddMemoryResponse {
    pub id: String,
    #[serde(default)]
    pub primary_sector: Option<String>,
    #[serde(default)]
    pub sectors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryMemoryRequest {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub k: Option<u32>, // default 5 server-side
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_score: Option<f64>,
}

/// OpenMemory query responses vary across deployments. We keep:
/// - raw json for artifact storage
/// - a ref-only view for safe output (ids + scores + content hash)
#[derive(Debug, Clone)]
pub struct QueryHitRef {
    pub id: String,
    pub score: Option<f64>,
    pub content_hash: String, // sha256:... of content/text bytes
}

#[derive(Debug, Clone)]
pub struct QueryMemoryParsed {
    pub raw: JsonValue,
    pub hits: Vec<QueryHitRef>,
}
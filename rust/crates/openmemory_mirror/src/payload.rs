use serde::{Deserialize, Serialize};
use serde_json::Value;

// OpenMemory Backend API:
// POST /memory/add
// { content, tags?, metadata?, user_id? }

#[derive(Debug, Clone, Serialize)]
pub struct AddMemoryRequest {
    pub content: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AddMemoryResponse {
    pub id: String,
    #[serde(default)]
    pub primary_sector: Option<String>,
    #[serde(default)]
    pub sectors: Vec<String>,
}

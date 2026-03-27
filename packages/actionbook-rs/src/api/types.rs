#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Search type for actions
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchType {
    Vector,
    Fulltext,
    #[default]
    Hybrid,
}

impl std::fmt::Display for SearchType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchType::Vector => write!(f, "vector"),
            SearchType::Fulltext => write!(f, "fulltext"),
            SearchType::Hybrid => write!(f, "hybrid"),
        }
    }
}

/// Parameters for searching actions (new text-based API)
#[derive(Debug, Default)]
pub struct SearchActionsParams {
    pub query: String,
    pub domain: Option<String>,
    pub background: Option<String>,
    pub url: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Parameters for legacy search actions (JSON API)
#[derive(Debug, Default)]
pub struct SearchActionsLegacyParams {
    pub query: String,
    pub search_type: Option<SearchType>,
    pub limit: Option<u32>,
    pub source_ids: Option<String>,
    pub min_score: Option<f64>,
}

/// A single search result
#[derive(Debug, Deserialize, Serialize)]
pub struct SearchResult {
    pub action_id: String,
    pub content: String,
    pub score: f64,
    #[serde(rename = "createdAt")]
    pub created_at: Option<String>,
}

/// Response from search actions API
#[derive(Debug, Deserialize)]
pub struct SearchActionsResponse {
    pub success: bool,
    pub query: String,
    pub results: Vec<SearchResult>,
    pub count: usize,
    pub total: usize,
    #[serde(rename = "hasMore")]
    pub has_more: bool,
}

/// Element selector information
#[derive(Debug, Deserialize, Serialize)]
pub struct ElementInfo {
    pub css_selector: Option<String>,
    pub xpath_selector: Option<String>,
    pub description: Option<String>,
    pub element_type: Option<String>,
    pub allow_methods: Option<Vec<String>>,
    pub depends_on: Option<String>,
}

/// Action detail response (elements is a JSON string from API)
#[derive(Debug, Deserialize, Serialize)]
pub struct ActionDetail {
    pub action_id: String,
    pub content: String,
    /// Elements come as a JSON string from the API
    #[serde(default, deserialize_with = "deserialize_elements")]
    pub elements: Option<HashMap<String, ElementInfo>>,
    #[serde(rename = "createdAt")]
    pub created_at: Option<String>,
    #[serde(rename = "documentId")]
    pub document_id: Option<i64>,
    #[serde(rename = "documentTitle")]
    pub document_title: Option<String>,
    #[serde(rename = "documentUrl")]
    pub document_url: Option<String>,
    #[serde(rename = "chunkIndex")]
    pub chunk_index: Option<i32>,
    pub heading: Option<String>,
    #[serde(rename = "tokenCount")]
    pub token_count: Option<i32>,
}

/// Deserialize elements from JSON string
fn deserialize_elements<'de, D>(
    deserializer: D,
) -> Result<Option<HashMap<String, ElementInfo>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;

    let value: Option<serde_json::Value> = Option::deserialize(deserializer)?;

    match value {
        None => Ok(None),
        Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(s)) => {
            // Parse the JSON string
            serde_json::from_str(&s)
                .map(Some)
                .map_err(|e| D::Error::custom(format!("Failed to parse elements: {}", e)))
        }
        Some(serde_json::Value::Object(map)) => {
            // Already an object, deserialize directly
            serde_json::from_value(serde_json::Value::Object(map))
                .map(Some)
                .map_err(|e| D::Error::custom(format!("Failed to parse elements: {}", e)))
        }
        _ => Err(D::Error::custom("Expected string or object for elements")),
    }
}

/// Source item
#[derive(Debug, Deserialize, Serialize)]
pub struct SourceItem {
    pub id: i64,
    pub name: String,
    #[serde(rename = "baseUrl")]
    pub base_url: String,
    pub description: Option<String>,
    pub domain: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(rename = "healthScore")]
    pub health_score: Option<f64>,
    #[serde(rename = "lastCrawledAt")]
    pub last_crawled_at: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: Option<String>,
}

/// Response from list sources API
#[derive(Debug, Deserialize)]
pub struct ListSourcesResponse {
    pub success: bool,
    pub results: Vec<SourceItem>,
    pub count: usize,
}

/// Response from search sources API
#[derive(Debug, Deserialize)]
pub struct SearchSourcesResponse {
    pub success: bool,
    pub query: String,
    pub results: Vec<SourceItem>,
    pub count: usize,
}

/// API error response
#[derive(Debug, Deserialize)]
pub struct ApiErrorResponse {
    pub message: String,
    pub code: Option<String>,
}

// ============================================
// Structured area action types (for execute/validate/act commands)
// ============================================

/// A single element within an area action
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AreaElement {
    pub css_selector: Option<String>,
    pub xpath_selector: Option<String>,
    pub description: Option<String>,
    pub element_type: Option<String>,
    #[serde(default)]
    pub allow_methods: Vec<String>,
}

/// Structured area action detail for execute/validate commands
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AreaActionDetail {
    pub area_id: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub elements: HashMap<String, AreaElement>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_type_default_is_hybrid() {
        assert_eq!(SearchType::default().to_string(), "hybrid");
    }

    #[test]
    fn search_type_display() {
        assert_eq!(SearchType::Vector.to_string(), "vector");
        assert_eq!(SearchType::Fulltext.to_string(), "fulltext");
        assert_eq!(SearchType::Hybrid.to_string(), "hybrid");
    }

    #[test]
    fn search_type_serde_round_trip() {
        for st in [SearchType::Vector, SearchType::Fulltext, SearchType::Hybrid] {
            let original = st.to_string();
            let json = serde_json::to_string(&st).unwrap();
            let decoded: SearchType = serde_json::from_str(&json).unwrap();
            assert_eq!(original, decoded.to_string());
        }
    }

    #[test]
    fn search_result_deserializes_correctly() {
        let json = r#"{
            "action_id": "site/github.com/page/home/element/search-btn",
            "content": "Search button on GitHub",
            "score": 0.95,
            "createdAt": "2024-01-01T00:00:00Z"
        }"#;
        let result: SearchResult = serde_json::from_str(json).unwrap();
        assert_eq!(
            result.action_id,
            "site/github.com/page/home/element/search-btn"
        );
        assert!((result.score - 0.95).abs() < 1e-9);
        assert_eq!(result.created_at.as_deref(), Some("2024-01-01T00:00:00Z"));
    }

    #[test]
    fn action_detail_with_elements_as_json_string() {
        // Construct JSON programmatically to avoid raw-string escaping issues.
        let elements_obj = serde_json::json!({"btn": {"css_selector": ".submit-btn"}});
        let elements_str = elements_obj.to_string();
        let payload = serde_json::json!({
            "action_id": "site/example.com/page/home/element/btn",
            "content": "A button",
            "elements": elements_str,
        });
        let detail: ActionDetail = serde_json::from_str(&payload.to_string()).unwrap();
        let elements = detail.elements.unwrap();
        let btn = elements.get("btn").unwrap();
        assert_eq!(btn.css_selector.as_deref(), Some(".submit-btn"));
    }

    #[test]
    fn action_detail_with_elements_as_null() {
        let json = r#"{
            "action_id": "site/example.com/page/home/element/btn",
            "content": "A button",
            "elements": null
        }"#;
        let detail: ActionDetail = serde_json::from_str(json).unwrap();
        assert!(detail.elements.is_none());
    }

    #[test]
    fn action_detail_with_elements_as_object() {
        let json = r#"{
            "action_id": "site/example.com/page/home/element/link",
            "content": "A link",
            "elements": {"link": {"xpath_selector": "//a[@id='main']"}}
        }"#;
        let detail: ActionDetail = serde_json::from_str(json).unwrap();
        let elements = detail.elements.unwrap();
        let link = elements.get("link").unwrap();
        assert_eq!(link.xpath_selector.as_deref(), Some("//a[@id='main']"));
    }

    #[test]
    fn search_actions_params_default() {
        let params = SearchActionsParams::default();
        assert!(params.query.is_empty());
        assert!(params.domain.is_none());
        assert!(params.background.is_none());
        assert!(params.url.is_none());
        assert!(params.page.is_none());
        assert!(params.page_size.is_none());
    }

    #[test]
    fn search_actions_legacy_params_default() {
        let params = SearchActionsLegacyParams::default();
        assert!(params.query.is_empty());
        assert!(params.search_type.is_none());
        assert!(params.limit.is_none());
        assert!(params.source_ids.is_none());
        assert!(params.min_score.is_none());
    }
}

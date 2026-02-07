use std::time::Duration;

use reqwest::{Client, StatusCode};

use super::types::*;
use crate::config::Config;
use crate::error::{ActionbookError, Result};

/// Actionbook API client
pub struct ApiClient {
    client: Client,
    base_url: String,
    api_key: Option<String>,
}

impl ApiClient {
    /// Create a new API client from config
    pub fn from_config(config: &Config) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| {
                ActionbookError::ApiError(format!("Failed to create HTTP client: {}", e))
            })?;

        Ok(Self {
            client,
            base_url: config.api.base_url.clone(),
            api_key: config.api.api_key.clone(),
        })
    }

    /// Build a request with common headers (JSON)
    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.client.request(method, &url);

        if let Some(ref key) = self.api_key {
            req = req.header("X-API-Key", key);
        }

        req.header("Content-Type", "application/json")
    }

    /// Build a request with common headers (Text)
    fn request_text(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.client.request(method, &url);

        if let Some(ref key) = self.api_key {
            req = req.header("X-API-Key", key);
        }

        req.header("Accept", "text/plain")
    }

    // ============================================
    // Text-based API methods (primary)
    // ============================================

    /// Search for actions (returns plain text)
    pub async fn search_actions(&self, params: SearchActionsParams) -> Result<String> {
        let mut query_params = vec![("query", params.query)];

        if let Some(domain) = params.domain {
            query_params.push(("domain", domain));
        }

        if let Some(background) = params.background {
            query_params.push(("background", background));
        }

        if let Some(url) = params.url {
            query_params.push(("url", url));
        }

        if let Some(page) = params.page {
            query_params.push(("page", page.to_string()));
        }

        if let Some(page_size) = params.page_size {
            query_params.push(("page_size", page_size.to_string()));
        }

        let response = self
            .request_text(reqwest::Method::GET, "/api/search_actions")
            .query(&query_params)
            .send()
            .await
            .map_err(|e| ActionbookError::ApiError(format!("Request failed: {}", e)))?;

        self.handle_text_response(response).await
    }

    /// Get action by area ID (returns plain text)
    pub async fn get_action_by_area_id(&self, area_id: &str) -> Result<String> {
        let response = self
            .request_text(reqwest::Method::GET, "/api/get_action_by_area_id")
            .query(&[("area_id", area_id)])
            .send()
            .await
            .map_err(|e| ActionbookError::ApiError(format!("Request failed: {}", e)))?;

        self.handle_text_response(response).await
    }

    // ============================================
    // Legacy JSON API methods (deprecated)
    // ============================================

    /// Search for actions (legacy JSON API)
    #[deprecated(note = "Use search_actions() instead")]
    #[allow(dead_code)]
    pub async fn search_actions_legacy(
        &self,
        params: SearchActionsLegacyParams,
    ) -> Result<SearchActionsResponse> {
        let mut query_params = vec![("q", params.query)];

        if let Some(search_type) = params.search_type {
            query_params.push(("type", search_type.to_string()));
        }

        if let Some(limit) = params.limit {
            query_params.push(("limit", limit.to_string()));
        }

        if let Some(source_ids) = params.source_ids {
            query_params.push(("sourceIds", source_ids));
        }

        if let Some(min_score) = params.min_score {
            query_params.push(("minScore", min_score.to_string()));
        }

        let response = self
            .request(reqwest::Method::GET, "/api/actions/search")
            .query(&query_params)
            .send()
            .await
            .map_err(|e| ActionbookError::ApiError(format!("Request failed: {}", e)))?;

        self.handle_response(response).await
    }

    /// Get action by ID (legacy JSON API)
    #[deprecated(note = "Use get_action_by_area_id() instead")]
    #[allow(dead_code)]
    pub async fn get_action(&self, id: &str) -> Result<ActionDetail> {
        let response = self
            .request(reqwest::Method::GET, "/api/actions")
            .query(&[("id", id)])
            .send()
            .await
            .map_err(|e| ActionbookError::ApiError(format!("Request failed: {}", e)))?;

        // API returns ActionDetail directly, not wrapped
        self.handle_response(response).await
    }

    /// List all sources
    pub async fn list_sources(&self, limit: Option<u32>) -> Result<ListSourcesResponse> {
        let mut query_params = vec![];

        if let Some(limit) = limit {
            query_params.push(("limit", limit.to_string()));
        }

        let response = self
            .request(reqwest::Method::GET, "/api/sources")
            .query(&query_params)
            .send()
            .await
            .map_err(|e| ActionbookError::ApiError(format!("Request failed: {}", e)))?;

        self.handle_response(response).await
    }

    /// Search sources
    pub async fn search_sources(
        &self,
        query: &str,
        limit: Option<u32>,
    ) -> Result<SearchSourcesResponse> {
        let mut query_params = vec![("q", query.to_string())];

        if let Some(limit) = limit {
            query_params.push(("limit", limit.to_string()));
        }

        let response = self
            .request(reqwest::Method::GET, "/api/sources/search")
            .query(&query_params)
            .send()
            .await
            .map_err(|e| ActionbookError::ApiError(format!("Request failed: {}", e)))?;

        self.handle_response(response).await
    }

    /// Handle API response (JSON)
    async fn handle_response<T: serde::de::DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<T> {
        let status = response.status();

        if status.is_success() {
            response
                .json()
                .await
                .map_err(|e| ActionbookError::ApiError(format!("Failed to parse response: {}", e)))
        } else {
            let error_msg = match status {
                StatusCode::NOT_FOUND => "Resource not found".to_string(),
                StatusCode::TOO_MANY_REQUESTS => {
                    "Rate limited. Please try again later.".to_string()
                }
                StatusCode::UNAUTHORIZED => "Invalid or missing API key".to_string(),
                _ => {
                    // Try to parse error response
                    match response.json::<ApiErrorResponse>().await {
                        Ok(err) => err.message,
                        Err(_) => format!("API error: {}", status),
                    }
                }
            };
            Err(ActionbookError::ApiError(error_msg))
        }
    }

    /// Handle API response (Text)
    async fn handle_text_response(&self, response: reqwest::Response) -> Result<String> {
        let status = response.status();

        if status.is_success() {
            response
                .text()
                .await
                .map_err(|e| ActionbookError::ApiError(format!("Failed to read response: {}", e)))
        } else {
            let error_msg = match status {
                StatusCode::NOT_FOUND => "Resource not found".to_string(),
                StatusCode::TOO_MANY_REQUESTS => {
                    "Rate limited. Please try again later.".to_string()
                }
                StatusCode::UNAUTHORIZED => "Invalid or missing API key".to_string(),
                _ => {
                    // Try to read error text
                    match response.text().await {
                        Ok(text) if !text.is_empty() => text,
                        _ => format!("API error: {}", status),
                    }
                }
            };
            Err(ActionbookError::ApiError(error_msg))
        }
    }
}

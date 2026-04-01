use std::time::Duration;

use reqwest::{Client, StatusCode};

use super::types::*;
use crate::config::ConfigFile;
use crate::error::CliError;

const DEFAULT_API_URL: &str = "https://api.actionbook.dev";

/// Actionbook API client
pub struct ApiClient {
    client: Client,
    base_url: String,
    api_key: Option<String>,
}

impl ApiClient {
    /// Create a new API client from config
    pub(crate) fn from_config(config: &ConfigFile) -> Result<Self, CliError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| CliError::ApiError(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            client,
            base_url: config
                .api
                .base_url
                .clone()
                .unwrap_or_else(|| DEFAULT_API_URL.to_string()),
            api_key: config.api.api_key.clone(),
        })
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

    /// Search for actions (returns plain text)
    pub async fn search_actions(&self, params: SearchActionsParams) -> Result<String, CliError> {
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
            .map_err(|e| CliError::ApiError(format!("Request failed: {}", e)))?;

        self.handle_text_response(response).await
    }

    /// Get action by area ID (returns plain text)
    pub async fn get_action_by_area_id(&self, area_id: &str) -> Result<String, CliError> {
        let response = self
            .request_text(reqwest::Method::GET, "/api/get_action_by_area_id")
            .query(&[("area_id", area_id)])
            .send()
            .await
            .map_err(|e| CliError::ApiError(format!("Request failed: {}", e)))?;

        self.handle_text_response(response).await
    }

    /// Handle API response (Text)
    async fn handle_text_response(&self, response: reqwest::Response) -> Result<String, CliError> {
        let status = response.status();

        if status.is_success() {
            response
                .text()
                .await
                .map_err(|e| CliError::ApiError(format!("Failed to read response: {}", e)))
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
            Err(CliError::ApiError(error_msg))
        }
    }
}

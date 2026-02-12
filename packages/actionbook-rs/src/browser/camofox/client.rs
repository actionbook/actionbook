//! REST API client for Camoufox browser server

use super::types::{
    ClickRequest, CreateTabRequest, CreateTabResponse, NavigateRequest, ScreenshotResponse,
    SnapshotResponse, TypeTextRequest,
};
use crate::error::{ActionbookError, Result};
use reqwest::{Client, StatusCode};
use std::time::Duration;

/// HTTP client for interacting with camofox-browser REST API
#[derive(Debug, Clone)]
pub struct CamofoxClient {
    base_url: String,
    client: Client,
    user_id: String,
}

impl CamofoxClient {
    /// Create a new Camoufox client
    ///
    /// # Arguments
    /// * `port` - Port number where camofox-browser is running (default: 9377)
    /// * `user_id` - Unique user identifier for this session
    pub fn new(port: u16, user_id: String) -> Self {
        let base_url = format!("http://localhost:{}", port);
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            base_url,
            client,
            user_id,
        }
    }

    /// Check if the Camoufox server is reachable
    pub async fn health_check(&self) -> Result<()> {
        let url = format!("{}/health", self.base_url);
        match self.client.get(&url).send().await {
            Ok(response) if response.status().is_success() => Ok(()),
            Ok(response) => Err(ActionbookError::CamofoxServerUnreachable(format!(
                "{} (status: {})",
                self.base_url,
                response.status()
            ))),
            Err(e) => Err(ActionbookError::CamofoxServerUnreachable(format!(
                "{} (error: {})",
                self.base_url, e
            ))),
        }
    }

    /// Create a new browser tab and navigate to URL
    pub async fn create_tab(
        &self,
        session_key: &str,
        url: &str,
    ) -> Result<CreateTabResponse> {
        let request_url = format!("{}/tabs", self.base_url);
        let body = CreateTabRequest {
            user_id: self.user_id.clone(),
            session_key: session_key.to_string(),
            url: url.to_string(),
        };

        let response = self
            .client
            .post(&request_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ActionbookError::BrowserOperation(format!("Failed to create tab: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(ActionbookError::BrowserOperation(format!(
                "Create tab failed with status {}: {}",
                status, error_text
            )));
        }

        response
            .json::<CreateTabResponse>()
            .await
            .map_err(|e| ActionbookError::BrowserOperation(format!("Failed to parse response: {}", e)))
    }

    /// Get accessibility tree snapshot for a tab
    pub async fn get_snapshot(&self, tab_id: &str) -> Result<SnapshotResponse> {
        let url = format!("{}/tabs/{}/snapshot", self.base_url, tab_id);

        let response = self
            .client
            .get(&url)
            .query(&[("user_id", &self.user_id)])
            .send()
            .await
            .map_err(|e| ActionbookError::BrowserOperation(format!("Failed to get snapshot: {}", e)))?;

        if response.status() == StatusCode::NOT_FOUND {
            return Err(ActionbookError::TabNotFound(tab_id.to_string()));
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(ActionbookError::BrowserOperation(format!(
                "Get snapshot failed with status {}: {}",
                status, error_text
            )));
        }

        response
            .json::<SnapshotResponse>()
            .await
            .map_err(|e| ActionbookError::BrowserOperation(format!("Failed to parse snapshot: {}", e)))
    }

    /// Click an element by its element reference
    pub async fn click(&self, tab_id: &str, element_ref: &str) -> Result<()> {
        let url = format!("{}/tabs/{}/click", self.base_url, tab_id);
        let body = ClickRequest {
            user_id: self.user_id.clone(),
            element_ref: element_ref.to_string(),
        };

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ActionbookError::BrowserOperation(format!("Failed to click: {}", e)))?;

        if response.status() == StatusCode::NOT_FOUND {
            return Err(ActionbookError::ElementNotFound(element_ref.to_string()));
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(ActionbookError::BrowserOperation(format!(
                "Click failed with status {}: {}",
                status, error_text
            )));
        }

        Ok(())
    }

    /// Type text into an element
    pub async fn type_text(&self, tab_id: &str, element_ref: &str, text: &str) -> Result<()> {
        let url = format!("{}/tabs/{}/type", self.base_url, tab_id);
        let body = TypeTextRequest {
            user_id: self.user_id.clone(),
            element_ref: element_ref.to_string(),
            text: text.to_string(),
        };

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ActionbookError::BrowserOperation(format!("Failed to type text: {}", e)))?;

        if response.status() == StatusCode::NOT_FOUND {
            return Err(ActionbookError::ElementNotFound(element_ref.to_string()));
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(ActionbookError::BrowserOperation(format!(
                "Type text failed with status {}: {}",
                status, error_text
            )));
        }

        Ok(())
    }

    /// Navigate to a URL
    pub async fn navigate(&self, tab_id: &str, url: &str) -> Result<()> {
        let request_url = format!("{}/tabs/{}/navigate", self.base_url, tab_id);
        let body = NavigateRequest {
            user_id: self.user_id.clone(),
            url: url.to_string(),
        };

        let response = self
            .client
            .post(&request_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ActionbookError::BrowserOperation(format!("Failed to navigate: {}", e)))?;

        if response.status() == StatusCode::NOT_FOUND {
            return Err(ActionbookError::TabNotFound(tab_id.to_string()));
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(ActionbookError::BrowserOperation(format!(
                "Navigate failed with status {}: {}",
                status, error_text
            )));
        }

        Ok(())
    }

    /// Take a screenshot of the current tab
    pub async fn screenshot(&self, tab_id: &str) -> Result<Vec<u8>> {
        let url = format!("{}/tabs/{}/screenshot", self.base_url, tab_id);

        let response = self
            .client
            .get(&url)
            .query(&[("user_id", &self.user_id)])
            .send()
            .await
            .map_err(|e| ActionbookError::BrowserOperation(format!("Failed to take screenshot: {}", e)))?;

        if response.status() == StatusCode::NOT_FOUND {
            return Err(ActionbookError::TabNotFound(tab_id.to_string()));
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(ActionbookError::BrowserOperation(format!(
                "Screenshot failed with status {}: {}",
                status, error_text
            )));
        }

        let screenshot_response = response
            .json::<ScreenshotResponse>()
            .await
            .map_err(|e| ActionbookError::BrowserOperation(format!("Failed to parse screenshot: {}", e)))?;

        // Decode base64 to bytes
        use base64::{engine::general_purpose, Engine as _};
        general_purpose::STANDARD
            .decode(&screenshot_response.data)
            .map_err(|e| ActionbookError::BrowserOperation(format!("Failed to decode screenshot: {}", e)))
    }

    /// Get the active tab ID for a session
    pub async fn get_active_tab(&self, session_key: &str) -> Result<Option<String>> {
        let url = format!("{}/sessions/{}/active-tab", self.base_url, session_key);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| ActionbookError::BrowserOperation(format!("Failed to get active tab: {}", e)))?;

        if response.status() == StatusCode::NOT_FOUND {
            // No active tab for this session
            return Ok(None);
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(ActionbookError::BrowserOperation(format!(
                "Get active tab failed with status {}: {}",
                status, error_text
            )));
        }

        #[derive(serde::Deserialize)]
        struct ActiveTabResponse {
            tab_id: String,
        }

        let active_tab_response = response
            .json::<ActiveTabResponse>()
            .await
            .map_err(|e| ActionbookError::BrowserOperation(format!("Failed to parse active tab response: {}", e)))?;

        Ok(Some(active_tab_response.tab_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = CamofoxClient::new(9377, "test-user".to_string());
        assert_eq!(client.base_url, "http://localhost:9377");
        assert_eq!(client.user_id, "test-user");
    }

    #[tokio::test]
    #[ignore] // Requires camofox-browser running
    async fn test_health_check() {
        let client = CamofoxClient::new(9377, "test-user".to_string());
        let result = client.health_check().await;
        assert!(result.is_ok(), "Health check should succeed when server is running");
    }

    #[tokio::test]
    #[ignore] // Requires camofox-browser running
    async fn test_create_tab() {
        let client = CamofoxClient::new(9377, "test-user".to_string());
        let result = client.create_tab("test-session", "https://example.com").await;
        assert!(result.is_ok(), "Create tab should succeed");

        let response = result.unwrap();
        assert!(!response.id.is_empty(), "Tab ID should not be empty");
        assert_eq!(response.url, "https://example.com");
    }
}

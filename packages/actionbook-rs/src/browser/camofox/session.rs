//! Camoufox session management with snapshot caching

use super::{client::CamofoxClient, snapshot::AccessibilityTreeExt, types::AccessibilityNode};
use crate::error::{ActionbookError, Result};
use std::time::{Duration, Instant};

/// Manages a Camoufox browser session with snapshot caching
#[derive(Debug)]
pub struct CamofoxSession {
    client: CamofoxClient,
    active_tab_id: Option<String>,
    session_key: String,
    snapshot_cache: Option<SnapshotCache>,
}

#[derive(Debug)]
struct SnapshotCache {
    tree: AccessibilityNode,
    timestamp: Instant,
    ttl: Duration,
}

impl SnapshotCache {
    fn new(tree: AccessibilityNode, ttl: Duration) -> Self {
        Self {
            tree,
            timestamp: Instant::now(),
            ttl,
        }
    }

    fn is_fresh(&self) -> bool {
        self.timestamp.elapsed() < self.ttl
    }
}

impl CamofoxSession {
    /// Connect to Camoufox server and create a new session
    ///
    /// # Arguments
    /// * `port` - Port number where camofox-browser is running
    /// * `user_id` - Unique user identifier
    /// * `session_key` - Session key for grouping tabs
    pub async fn connect(port: u16, user_id: String, session_key: String) -> Result<Self> {
        let client = CamofoxClient::new(port, user_id);

        // Verify server is reachable
        client.health_check().await?;

        Ok(Self {
            client,
            active_tab_id: None,
            session_key,
            snapshot_cache: None,
        })
    }

    /// Create a new tab and navigate to URL
    pub async fn create_tab(&mut self, url: &str) -> Result<String> {
        let response = self.client.create_tab(&self.session_key, url).await?;
        self.active_tab_id = Some(response.id.clone());

        // Invalidate cache when creating new tab
        self.snapshot_cache = None;

        Ok(response.id)
    }

    /// Get the active tab ID
    pub fn active_tab(&self) -> Result<&str> {
        self.active_tab_id
            .as_deref()
            .ok_or_else(|| ActionbookError::BrowserOperation("No active tab".to_string()))
    }

    /// Refresh the accessibility tree snapshot
    pub async fn refresh_snapshot(&mut self) -> Result<()> {
        let tab_id = self.active_tab()?;
        let response = self.client.get_snapshot(tab_id).await?;

        // Cache for 5 seconds by default
        self.snapshot_cache = Some(SnapshotCache::new(response.tree, Duration::from_secs(5)));

        Ok(())
    }

    /// Resolve a CSS selector to an element reference
    ///
    /// Supports:
    /// - Element refs: "e1", "e2", etc. (returned as-is)
    /// - CSS selectors: "#login", ".btn-primary", "button", etc. (resolved via snapshot)
    pub async fn resolve_selector(&mut self, selector: &str) -> Result<String> {
        // Phase 1: Check if already an element ref (e1, e2, etc.)
        if selector.starts_with('e') && selector[1..].parse::<u32>().is_ok() {
            return Ok(selector.to_string());
        }

        // Phase 2: Try cache lookup
        if let Some(cache) = &self.snapshot_cache {
            if cache.is_fresh() {
                if let Some(element_ref) = cache.tree.find_matching(selector) {
                    return Ok(element_ref.to_string());
                }
            }
        }

        // Phase 3: Fetch fresh snapshot
        self.refresh_snapshot().await?;

        // Phase 4: Search in fresh snapshot
        self.snapshot_cache
            .as_ref()
            .and_then(|c| c.tree.find_matching(selector))
            .map(|s| s.to_string())
            .ok_or_else(|| {
                ActionbookError::ElementRefResolution(
                    selector.to_string(),
                    "Element not found in accessibility tree".to_string(),
                )
            })
    }

    /// Click an element by selector or element ref
    pub async fn click(&mut self, selector: &str) -> Result<()> {
        let element_ref = self.resolve_selector(selector).await?;
        let tab_id = self.active_tab()?.to_string();
        self.client.click(&tab_id, &element_ref).await?;

        // Invalidate cache after interaction
        self.snapshot_cache = None;

        Ok(())
    }

    /// Type text into an element
    pub async fn type_text(&mut self, selector: &str, text: &str) -> Result<()> {
        let element_ref = self.resolve_selector(selector).await?;
        let tab_id = self.active_tab()?.to_string();
        self.client.type_text(&tab_id, &element_ref, text).await?;

        // Invalidate cache after interaction
        self.snapshot_cache = None;

        Ok(())
    }

    /// Navigate to a URL
    pub async fn navigate(&mut self, url: &str) -> Result<()> {
        let tab_id = self.active_tab()?.to_string();
        self.client.navigate(&tab_id, url).await?;

        // Invalidate cache after navigation
        self.snapshot_cache = None;

        Ok(())
    }

    /// Take a screenshot
    pub async fn screenshot(&self) -> Result<Vec<u8>> {
        let tab_id = self.active_tab()?;
        self.client.screenshot(tab_id).await
    }

    /// Get the current accessibility tree as a string (for debugging/inspection)
    pub async fn get_content(&mut self) -> Result<String> {
        self.refresh_snapshot().await?;

        if let Some(cache) = &self.snapshot_cache {
            // Convert to JSON for readability
            serde_json::to_string_pretty(&cache.tree)
                .map_err(|e| ActionbookError::BrowserOperation(format!("Failed to serialize tree: {}", e)))
        } else {
            Err(ActionbookError::BrowserOperation(
                "No snapshot available".to_string(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_element_ref_passthrough() {
        // Element refs should be returned as-is without network calls
        let selector = "e1";
        assert!(selector.starts_with('e') && selector[1..].parse::<u32>().is_ok());

        let selector = "e42";
        assert!(selector.starts_with('e') && selector[1..].parse::<u32>().is_ok());

        let selector = "#login";
        assert!(!(selector.starts_with('e') && selector.get(1..).and_then(|s| s.parse::<u32>().ok()).is_some()));
    }

    #[tokio::test]
    #[ignore] // Requires camofox-browser running
    async fn test_session_connect() {
        let result = CamofoxSession::connect(
            9377,
            "test-user".to_string(),
            "test-session".to_string(),
        )
        .await;

        assert!(result.is_ok(), "Should connect to Camoufox server");
    }

    #[tokio::test]
    #[ignore] // Requires camofox-browser running
    async fn test_create_tab_and_interact() {
        let mut session = CamofoxSession::connect(
            9377,
            "test-user".to_string(),
            "test-session".to_string(),
        )
        .await
        .unwrap();

        let tab_id = session.create_tab("https://example.com").await.unwrap();
        assert!(!tab_id.is_empty());

        // Test snapshot fetching
        let result = session.refresh_snapshot().await;
        assert!(result.is_ok(), "Should fetch snapshot");

        // Test getting content
        let content = session.get_content().await.unwrap();
        assert!(!content.is_empty());
    }
}

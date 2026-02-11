use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use serde_json::Value;
use tokio::time::timeout;

use super::backend::{BrowserBackend, OpenResult, PageEntry};
use super::SessionManager;
use crate::error::{ActionbookError, Result};

/// Isolated mode backend: dedicated debug browser controlled via CDP.
///
/// Wraps `SessionManager` and delegates each operation to the corresponding
/// SessionManager method, using a persisted profile name.
pub struct IsolatedBackend {
    session_manager: SessionManager,
    profile_name: String,
}

impl IsolatedBackend {
    pub fn new(session_manager: SessionManager, profile_name: String) -> Self {
        Self {
            session_manager,
            profile_name,
        }
    }

    fn profile_arg(&self) -> Option<&str> {
        Some(self.profile_name.as_str())
    }
}

#[async_trait]
impl BrowserBackend for IsolatedBackend {
    async fn open(&self, url: &str) -> Result<OpenResult> {
        let (browser, mut handler) = self
            .session_manager
            .get_or_create_session(self.profile_arg())
            .await?;

        tokio::spawn(async move { while handler.next().await.is_some() {} });

        let page = match timeout(Duration::from_secs(30), browser.new_page(url)).await {
            Ok(Ok(page)) => page,
            Ok(Err(e)) => {
                return Err(ActionbookError::Other(format!(
                    "Failed to open page: {}",
                    e
                )));
            }
            Err(_) => {
                return Err(ActionbookError::Timeout(format!(
                    "Page load timed out after 30 seconds: {}",
                    url
                )));
            }
        };

        // Apply stealth if feature enabled
        #[cfg(feature = "stealth")]
        if let Some(profile) = self.session_manager.get_stealth_profile() {
            if let Err(e) = super::apply_stealth_to_page(&page, profile).await {
                tracing::warn!("Failed to apply stealth profile: {}", e);
            }
        }

        let _ = timeout(Duration::from_secs(30), page.wait_for_navigation()).await;

        let title = match timeout(Duration::from_secs(5), page.get_title()).await {
            Ok(Ok(Some(t))) => t,
            _ => String::new(),
        };

        Ok(OpenResult {
            title,
            url: url.to_string(),
        })
    }

    async fn close(&self) -> Result<()> {
        self.session_manager
            .close_session(self.profile_arg())
            .await
    }

    async fn restart(&self) -> Result<()> {
        self.session_manager
            .close_session(self.profile_arg())
            .await?;

        // Trigger a new session creation
        let (_browser, mut handler) = self
            .session_manager
            .get_or_create_session(self.profile_arg())
            .await?;

        tokio::spawn(async move { while handler.next().await.is_some() {} });

        Ok(())
    }

    async fn goto(&self, url: &str) -> Result<()> {
        self.session_manager
            .goto(self.profile_arg(), url)
            .await
    }

    async fn back(&self) -> Result<()> {
        self.session_manager.go_back(self.profile_arg()).await
    }

    async fn forward(&self) -> Result<()> {
        self.session_manager.go_forward(self.profile_arg()).await
    }

    async fn reload(&self) -> Result<()> {
        self.session_manager.reload(self.profile_arg()).await
    }

    async fn pages(&self) -> Result<Vec<PageEntry>> {
        let pages = self
            .session_manager
            .get_pages(self.profile_arg())
            .await?;

        Ok(pages
            .into_iter()
            .map(|p| PageEntry {
                id: p.id,
                title: p.title,
                url: p.url,
            })
            .collect())
    }

    async fn switch(&self, _page_id: &str) -> Result<()> {
        // Isolated mode doesn't have direct tab switching via SessionManager.
        // This is a no-op / unsupported placeholder.
        Err(ActionbookError::Other(
            "Tab switching is not supported in isolated mode. Use page IDs from 'pages' command."
                .to_string(),
        ))
    }

    async fn wait_for(&self, selector: &str, timeout_ms: u64) -> Result<()> {
        self.session_manager
            .wait_for_element(self.profile_arg(), selector, timeout_ms)
            .await
    }

    async fn wait_nav(&self, timeout_ms: u64) -> Result<String> {
        self.session_manager
            .wait_for_navigation(self.profile_arg(), timeout_ms)
            .await
    }

    async fn click(&self, selector: &str, wait_ms: u64) -> Result<()> {
        if wait_ms > 0 {
            self.session_manager
                .wait_for_element(self.profile_arg(), selector, wait_ms)
                .await?;
        }
        self.session_manager
            .click_on_page(self.profile_arg(), selector)
            .await
    }

    async fn type_text(&self, selector: &str, text: &str, wait_ms: u64) -> Result<()> {
        if wait_ms > 0 {
            self.session_manager
                .wait_for_element(self.profile_arg(), selector, wait_ms)
                .await?;
        }
        self.session_manager
            .type_on_page(self.profile_arg(), selector, text)
            .await
    }

    async fn fill(&self, selector: &str, text: &str, wait_ms: u64) -> Result<()> {
        if wait_ms > 0 {
            self.session_manager
                .wait_for_element(self.profile_arg(), selector, wait_ms)
                .await?;
        }
        self.session_manager
            .fill_on_page(self.profile_arg(), selector, text)
            .await
    }

    async fn select(&self, selector: &str, value: &str) -> Result<()> {
        self.session_manager
            .select_on_page(self.profile_arg(), selector, value)
            .await
    }

    async fn hover(&self, selector: &str) -> Result<()> {
        self.session_manager
            .hover_on_page(self.profile_arg(), selector)
            .await
    }

    async fn focus(&self, selector: &str) -> Result<()> {
        self.session_manager
            .focus_on_page(self.profile_arg(), selector)
            .await
    }

    async fn press(&self, key: &str) -> Result<()> {
        self.session_manager
            .press_key(self.profile_arg(), key)
            .await
    }

    async fn screenshot(&self, full_page: bool) -> Result<Vec<u8>> {
        if full_page {
            self.session_manager
                .screenshot_full_page(self.profile_arg())
                .await
        } else {
            self.session_manager
                .screenshot_page(self.profile_arg())
                .await
        }
    }

    async fn pdf(&self) -> Result<Vec<u8>> {
        self.session_manager
            .pdf_page(self.profile_arg())
            .await
    }

    async fn eval(&self, code: &str) -> Result<Value> {
        self.session_manager
            .eval_on_page(self.profile_arg(), code)
            .await
    }

    async fn html(&self, selector: Option<&str>) -> Result<String> {
        self.session_manager
            .get_html(self.profile_arg(), selector)
            .await
    }

    async fn text(&self, selector: Option<&str>) -> Result<String> {
        self.session_manager
            .get_text(self.profile_arg(), selector)
            .await
    }

    async fn snapshot(&self) -> Result<Value> {
        // Snapshot uses the same JS in both modes — delegate to eval_on_page
        // with the snapshot JS. The actual JS is shared and will be called
        // from browser.rs during Phase 3 refactor.
        // For now, provide a placeholder that the refactor will fill.
        Err(ActionbookError::Other(
            "snapshot: delegate to eval with shared JS (Phase 3)".to_string(),
        ))
    }

    async fn inspect(&self, x: f64, y: f64) -> Result<Value> {
        self.session_manager
            .inspect_at(self.profile_arg(), x, y)
            .await
    }

    async fn viewport(&self) -> Result<(u32, u32)> {
        let (w, h) = self
            .session_manager
            .get_viewport(self.profile_arg())
            .await?;
        Ok((w as u32, h as u32))
    }

    async fn get_cookies(&self) -> Result<Vec<Value>> {
        self.session_manager
            .get_cookies(self.profile_arg())
            .await
    }

    async fn set_cookie(&self, name: &str, value: &str, domain: Option<&str>) -> Result<()> {
        self.session_manager
            .set_cookie(self.profile_arg(), name, value, domain)
            .await
    }

    async fn delete_cookie(&self, name: &str) -> Result<()> {
        self.session_manager
            .delete_cookie(self.profile_arg(), name)
            .await
    }

    async fn clear_cookies(&self, _domain: Option<&str>) -> Result<()> {
        self.session_manager
            .clear_cookies(self.profile_arg())
            .await
    }
}

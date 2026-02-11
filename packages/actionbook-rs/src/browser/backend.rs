use async_trait::async_trait;
use serde_json::Value;

use crate::error::Result;

/// Result of opening a new page/tab.
pub struct OpenResult {
    pub title: String,
    pub url: String,
}

/// A page/tab entry from the browser.
pub struct PageEntry {
    pub id: String,
    pub title: String,
    pub url: String,
}

/// Abstraction over browser control modes, eliminating if/else branching in commands.
///
/// Two implementations:
/// - `IsolatedBackend`: dedicated debug browser via CDP (wraps SessionManager)
/// - `ExtensionBackend`: user's Chrome via extension bridge
#[async_trait]
pub trait BrowserBackend: Send + Sync {
    // --- Lifecycle ---

    /// Open a new page/tab at the given URL.
    async fn open(&self, url: &str) -> Result<OpenResult>;

    /// Close the browser session (isolated) or detach the tab (extension).
    async fn close(&self) -> Result<()>;

    /// Restart: close + reopen (isolated) or reload (extension).
    async fn restart(&self) -> Result<()>;

    // --- Navigation ---

    async fn goto(&self, url: &str) -> Result<()>;
    async fn back(&self) -> Result<()>;
    async fn forward(&self) -> Result<()>;
    async fn reload(&self) -> Result<()>;

    // --- Page management ---

    async fn pages(&self) -> Result<Vec<PageEntry>>;
    async fn switch(&self, page_id: &str) -> Result<()>;

    // --- Waiting ---

    async fn wait_for(&self, selector: &str, timeout_ms: u64) -> Result<()>;
    async fn wait_nav(&self, timeout_ms: u64) -> Result<String>;

    // --- Interaction ---

    async fn click(&self, selector: &str, wait_ms: u64) -> Result<()>;
    async fn type_text(&self, selector: &str, text: &str, wait_ms: u64) -> Result<()>;
    async fn fill(&self, selector: &str, text: &str, wait_ms: u64) -> Result<()>;
    async fn select(&self, selector: &str, value: &str) -> Result<()>;
    async fn hover(&self, selector: &str) -> Result<()>;
    async fn focus(&self, selector: &str) -> Result<()>;
    async fn press(&self, key: &str) -> Result<()>;

    // --- Content extraction ---

    /// Take a screenshot, returning raw PNG bytes.
    async fn screenshot(&self, full_page: bool) -> Result<Vec<u8>>;

    /// Export the page as PDF, returning raw PDF bytes.
    async fn pdf(&self) -> Result<Vec<u8>>;

    /// Evaluate JavaScript and return the result.
    async fn eval(&self, code: &str) -> Result<Value>;

    /// Get page HTML (optionally scoped to a selector).
    async fn html(&self, selector: Option<&str>) -> Result<String>;

    /// Get page text content (optionally scoped to a selector).
    async fn text(&self, selector: Option<&str>) -> Result<String>;

    /// Get an accessibility snapshot of the page.
    async fn snapshot(&self) -> Result<Value>;

    /// Inspect the DOM element at viewport coordinates.
    async fn inspect(&self, x: f64, y: f64) -> Result<Value>;

    /// Get viewport dimensions (width, height).
    async fn viewport(&self) -> Result<(u32, u32)>;

    // --- Cookies ---

    async fn get_cookies(&self) -> Result<Vec<Value>>;
    async fn set_cookie(&self, name: &str, value: &str, domain: Option<&str>) -> Result<()>;
    async fn delete_cookie(&self, name: &str) -> Result<()>;
    async fn clear_cookies(&self, domain: Option<&str>) -> Result<()>;
}

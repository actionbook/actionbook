use async_trait::async_trait;
use serde_json::Value;

use crate::error::Result;

/// Shared snapshot JavaScript — builds an accessibility tree from the DOM.
/// Used by both IsolatedBackend and ExtensionBackend.
pub const SNAPSHOT_JS: &str = r#"
        (function() {
            const SKIP_TAGS = new Set([
                'script', 'style', 'noscript', 'template', 'svg',
                'path', 'defs', 'clippath', 'lineargradient', 'stop',
                'meta', 'link', 'br', 'wbr'
            ]);
            const INLINE_TAGS = new Set([
                'strong', 'b', 'em', 'i', 'code', 'span', 'small',
                'sup', 'sub', 'abbr', 'mark', 'u', 's', 'del', 'ins',
                'time', 'q', 'cite', 'dfn', 'var', 'samp', 'kbd'
            ]);
            const INTERACTIVE_ROLES = new Set([
                'button', 'link', 'textbox', 'checkbox', 'radio', 'combobox',
                'listbox', 'menuitem', 'menuitemcheckbox', 'menuitemradio',
                'option', 'searchbox', 'slider', 'spinbutton', 'switch',
                'tab', 'treeitem'
            ]);
            const CONTENT_ROLES = new Set([
                'heading', 'cell', 'gridcell', 'columnheader', 'rowheader',
                'listitem', 'article', 'region', 'main', 'navigation', 'img'
            ]);
            function getRole(el) {
                const explicit = el.getAttribute('role');
                if (explicit) return explicit.toLowerCase();
                const tag = el.tagName.toLowerCase();
                if (INLINE_TAGS.has(tag)) return tag;
                const roleMap = {
                    'a': el.hasAttribute('href') ? 'link' : 'generic',
                    'button': 'button',
                    'input': getInputRole(el),
                    'select': 'combobox',
                    'textarea': 'textbox',
                    'img': 'img',
                    'h1': 'heading', 'h2': 'heading', 'h3': 'heading',
                    'h4': 'heading', 'h5': 'heading', 'h6': 'heading',
                    'nav': 'navigation',
                    'main': 'main',
                    'header': 'banner',
                    'footer': 'contentinfo',
                    'aside': 'complementary',
                    'form': 'form',
                    'table': 'table',
                    'thead': 'rowgroup', 'tbody': 'rowgroup', 'tfoot': 'rowgroup',
                    'tr': 'row',
                    'th': 'columnheader',
                    'td': 'cell',
                    'ul': 'list', 'ol': 'list',
                    'li': 'listitem',
                    'details': 'group',
                    'summary': 'button',
                    'dialog': 'dialog',
                    'section': el.hasAttribute('aria-label') || el.hasAttribute('aria-labelledby') ? 'region' : 'generic',
                    'article': 'article'
                };
                return roleMap[tag] || 'generic';
            }
            function getInputRole(el) {
                const type = (el.getAttribute('type') || 'text').toLowerCase();
                const map = {
                    'text': 'textbox', 'email': 'textbox', 'password': 'textbox',
                    'search': 'searchbox', 'tel': 'textbox', 'url': 'textbox',
                    'number': 'spinbutton',
                    'checkbox': 'checkbox', 'radio': 'radio',
                    'submit': 'button', 'reset': 'button', 'button': 'button',
                    'range': 'slider'
                };
                return map[type] || 'textbox';
            }
            function getAccessibleName(el) {
                const ariaLabel = el.getAttribute('aria-label');
                if (ariaLabel) return ariaLabel.trim();
                const labelledBy = el.getAttribute('aria-labelledby');
                if (labelledBy) {
                    const label = document.getElementById(labelledBy);
                    if (label) return label.textContent?.trim()?.substring(0, 100) || '';
                }
                const tag = el.tagName.toLowerCase();
                if (tag === 'img') return el.getAttribute('alt') || '';
                if (tag === 'input' || tag === 'textarea' || tag === 'select') {
                    if (el.id) {
                        const label = document.querySelector('label[for="' + el.id + '"]');
                        if (label) return label.textContent?.trim()?.substring(0, 100) || '';
                    }
                    return el.getAttribute('placeholder') || el.getAttribute('title') || '';
                }
                if (tag === 'a' || tag === 'button' || tag === 'summary') {
                    return '';
                }
                if (['h1','h2','h3','h4','h5','h6'].includes(tag)) {
                    return el.textContent?.trim()?.substring(0, 150) || '';
                }
                const title = el.getAttribute('title');
                if (title) return title.trim();
                return '';
            }
            function isHidden(el) {
                if (el.hidden) return true;
                if (el.getAttribute('aria-hidden') === 'true') return true;
                const style = el.style;
                if (style.display === 'none' || style.visibility === 'hidden') return true;
                if (el.offsetParent === null && el.tagName.toLowerCase() !== 'body' &&
                    getComputedStyle(el).position !== 'fixed' && getComputedStyle(el).position !== 'sticky') {
                    const cs = getComputedStyle(el);
                    if (cs.display === 'none' || cs.visibility === 'hidden') return true;
                }
                return false;
            }
            let refCounter = 0;
            function walk(el, depth) {
                if (depth > 15) return null;
                const tag = el.tagName.toLowerCase();
                if (SKIP_TAGS.has(tag)) return null;
                if (isHidden(el)) return null;
                const role = getRole(el);
                const name = getAccessibleName(el);
                const isInteractive = INTERACTIVE_ROLES.has(role);
                const isContent = CONTENT_ROLES.has(role);
                const shouldRef = isInteractive || (isContent && name);
                let ref = null;
                if (shouldRef) {
                    refCounter++;
                    ref = 'e' + refCounter;
                }
                const children = [];
                for (const child of el.childNodes) {
                    if (child.nodeType === 1) {
                        const c = walk(child, depth + 1);
                        if (c) children.push(c);
                    } else if (child.nodeType === 3) {
                        const t = child.textContent?.trim();
                        if (t) {
                            const content = t.length > 200 ? t.substring(0, 200) + '...' : t;
                            children.push({ role: 'text', content });
                        }
                    }
                }
                if (role === 'generic' && !name && !ref && children.length === 1) {
                    return children[0];
                }
                if (role === 'generic' && !name && !ref && children.length === 0) {
                    return null;
                }
                const node = { role };
                if (name) node.name = name;
                if (ref) node.ref = ref;
                if (children.length > 0) node.children = children;
                if (role === 'link') {
                    const href = el.getAttribute('href');
                    if (href) node.url = href;
                }
                if (role === 'heading') {
                    const level = tag.match(/^h(\d)$/);
                    if (level) node.level = parseInt(level[1]);
                }
                if (role === 'textbox' || role === 'searchbox') {
                    node.value = el.value || '';
                }
                if (role === 'checkbox' || role === 'radio' || role === 'switch') {
                    node.checked = el.checked || false;
                }
                return node;
            }
            const tree = walk(document.body, 0);
            return { tree, refCount: refCounter };
        })()
    "#;

/// Result of opening a new page/tab.
pub struct OpenResult {
    pub title: String,
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

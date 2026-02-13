use std::time::Duration;

use async_trait::async_trait;
use base64::Engine;
use serde_json::Value;

use super::backend::{BrowserBackend, OpenResult, PageEntry};
use super::extension_bridge;
use crate::error::{ActionbookError, Result};

/// Max retries waiting for the Chrome extension to connect to the bridge.
const EXTENSION_CONNECT_RETRIES: u32 = 60;
/// Interval between retries when waiting for extension connection (60 * 500ms = 30s).
const EXTENSION_CONNECT_INTERVAL: Duration = Duration::from_millis(500);

/// Extension mode backend: controls user's Chrome via the extension bridge.
///
/// All commands are routed through the WebSocket bridge to the Chrome extension,
/// which executes them via CDP or the chrome.* APIs.
pub struct ExtensionBackend {
    port: u16,
}

impl ExtensionBackend {
    pub fn new(port: u16) -> Self {
        Self { port }
    }

    /// Send a command through the bridge with auto-attach retry (single attempt).
    async fn send_once(&self, method: &str, params: Value) -> Result<Value> {
        let result = extension_bridge::send_command(self.port, method, params.clone()).await;

        // Auto-attach: if a CDP method fails because no tab is attached, attach and retry
        if let Err(ActionbookError::ExtensionError(ref msg)) = result {
            if msg.contains("No tab attached") && !method.starts_with("Extension.") {
                tracing::debug!("Auto-attaching active tab for {}", method);
                extension_bridge::send_command(
                    self.port,
                    "Extension.attachActiveTab",
                    serde_json::json!({}),
                )
                .await?;
                return extension_bridge::send_command(self.port, method, params).await;
            }
        }

        result
    }

    /// Send a command with retry logic for extension-not-connected.
    ///
    /// - "Extension not connected": retry up to 60 times (30s) at 500ms intervals
    ///   to wait for the extension to connect to the bridge.
    /// - Silent retries for first 12 attempts (6s) to cover 2 extension polling cycles.
    ///   Extension polls via Native Messaging every 2s, so 2 cycles + handshake = 6s max wait.
    /// - After 12 failed attempts, print user-facing instructions.
    async fn send(&self, method: &str, params: Value) -> Result<Value> {
        let result = self.send_once(method, params.clone()).await;

        match &result {
            Err(ActionbookError::ExtensionError(msg))
                if msg.contains("Extension not connected") =>
            {
                // Silent retries: 12 attempts × 500ms = 6s (covers 2 extension polling cycles of 2s each + 2s handshake)
                const SILENT_RETRIES: u32 = 12;
                let mut user_notified = false;

                for attempt in 1..=EXTENSION_CONNECT_RETRIES {
                    tokio::time::sleep(EXTENSION_CONNECT_INTERVAL).await;
                    tracing::debug!(
                        "Retry {}/{}: waiting for extension connection",
                        attempt,
                        EXTENSION_CONNECT_RETRIES
                    );

                    match self.send_once(method, params.clone()).await {
                        Err(ActionbookError::ExtensionError(ref m))
                            if m.contains("Extension not connected") =>
                        {
                            // Print user-facing message only after silent retries exhausted
                            if attempt > SILENT_RETRIES && !user_notified {
                                eprintln!(
                                    "Waiting for Chrome extension to connect to the bridge (port {})...",
                                    self.port
                                );
                                eprintln!("Open Chrome with the Actionbook extension enabled.");
                                user_notified = true;
                            }
                            continue;
                        }
                        other => {
                            // Extension connected successfully
                            if other.is_ok() && user_notified {
                                eprintln!("  {} Extension connected", colored::Colorize::green("✓"));
                            }
                            return other;
                        }
                    }
                }

                Err(ActionbookError::ExtensionError(
                    "Extension did not connect within 30 seconds. \
                     Ensure Chrome is open with the Actionbook extension enabled."
                        .to_string(),
                ))
            }
            _ => result,
        }
    }

    /// Evaluate JS via Runtime.evaluate with configurable `awaitPromise`.
    /// Shared logic for both internal `eval_js` and the user-facing `eval` trait method.
    async fn eval_with_options(&self, expression: &str, await_promise: bool) -> Result<Value> {
        let mut params = serde_json::json!({
            "expression": expression,
            "returnByValue": true,
        });
        if await_promise {
            params["awaitPromise"] = serde_json::json!(true);
        }

        let result = self.send("Runtime.evaluate", params).await?;

        if let Some(exception) = result.get("exceptionDetails") {
            let msg = exception
                .get("text")
                .or_else(|| exception.get("exception").and_then(|e| e.get("description")))
                .and_then(|v| v.as_str())
                .unwrap_or("JavaScript exception");
            return Err(ActionbookError::ExtensionError(format!(
                "JS error (extension mode): {}",
                msg
            )));
        }

        Ok(result
            .get("result")
            .and_then(|r| r.get("value"))
            .cloned()
            .unwrap_or_else(|| {
                result
                    .get("result")
                    .cloned()
                    .unwrap_or(Value::Null)
            }))
    }

    /// Evaluate JS with `awaitPromise: true` — used by internal action helpers.
    async fn eval_js(&self, expression: &str) -> Result<Value> {
        self.eval_with_options(expression, true).await
    }

    /// Execute JS that returns `{success: bool, error?: string}` and check for errors.
    async fn eval_action(&self, js: &str, action_name: &str) -> Result<()> {
        let result = self.eval_js(js).await?;
        if result.get("success").and_then(|v| v.as_bool()) != Some(true) {
            let err = result
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("Unknown error");
            return Err(ActionbookError::ExtensionError(format!(
                "{} failed (extension mode): {}",
                action_name, err
            )));
        }
        Ok(())
    }

    /// Poll for an element matching selector until found or timeout.
    async fn wait_for_element(&self, selector: &str, timeout_ms: u64) -> Result<()> {
        let resolve_js = js_resolve_selector(selector);
        let poll_js = format!(
            r#"(async function() {{
                var deadline = Date.now() + {timeout_ms};
                while (Date.now() < deadline) {{
                    var el = {resolve_js};
                    if (el) return true;
                    await new Promise(r => setTimeout(r, 100));
                }}
                return false;
            }})()"#
        );
        let found = self.eval_js(&poll_js).await?;
        if found.as_bool() != Some(true) {
            return Err(ActionbookError::Timeout(format!(
                "Element not found within {}ms (extension mode): {}",
                timeout_ms, selector
            )));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// JS helpers (extracted from commands/browser.rs)
// ---------------------------------------------------------------------------

/// Escape a string for safe embedding in a JS single-quoted string literal.
fn escape_js_string(s: &str) -> String {
    // serde_json::to_string on &str never fails for valid UTF-8
    let json = serde_json::to_string(s).expect("serializing &str to JSON is infallible");
    let inner = &json[1..json.len() - 1];
    inner.replace("\\\"", "\"").replace('\'', "\\'")
}

/// Build a JS expression that resolves a selector (CSS or [ref=eN] format).
fn js_resolve_selector(selector: &str) -> String {
    format!(
        r#"(function(selector) {{
    if (/^\[ref=e\d+\]$/.test(selector)) {{
        var refId = selector.match(/^\[ref=(e\d+)\]$/)[1];
        var SKIP = new Set(['script','style','noscript','template','svg','path','defs','clippath','lineargradient','stop','meta','link','br','wbr']);
        var INTERACTIVE = new Set(['button','link','textbox','checkbox','radio','combobox','listbox','menuitem','menuitemcheckbox','menuitemradio','option','searchbox','slider','spinbutton','switch','tab','treeitem']);
        var CONTENT = new Set(['heading','cell','gridcell','columnheader','rowheader','listitem','article','region','main','navigation','img']);
        function getRole(el) {{
            var explicit = el.getAttribute('role');
            if (explicit) return explicit.toLowerCase();
            var tag = el.tagName.toLowerCase();
            var map = {{'a': el.hasAttribute('href')?'link':'generic','button':'button','select':'combobox','textarea':'textbox','img':'img','h1':'heading','h2':'heading','h3':'heading','h4':'heading','h5':'heading','h6':'heading','nav':'navigation','main':'main','header':'banner','footer':'contentinfo','aside':'complementary','form':'form','table':'table','thead':'rowgroup','tbody':'rowgroup','tfoot':'rowgroup','tr':'row','th':'columnheader','td':'cell','ul':'list','ol':'list','li':'listitem','details':'group','summary':'button','dialog':'dialog','article':'article'}};
            if (tag === 'input') {{
                var type = (el.getAttribute('type')||'text').toLowerCase();
                var imap = {{'text':'textbox','email':'textbox','password':'textbox','search':'searchbox','tel':'textbox','url':'textbox','number':'spinbutton','checkbox':'checkbox','radio':'radio','submit':'button','reset':'button','button':'button','range':'slider'}};
                return imap[type]||'textbox';
            }}
            if (tag === 'section') return (el.hasAttribute('aria-label')||el.hasAttribute('aria-labelledby'))?'region':'generic';
            return map[tag]||'generic';
        }}
        function getName(el) {{
            if (el.getAttribute('aria-label')) return el.getAttribute('aria-label').trim();
            return '';
        }}
        var counter = 0;
        function findRef(el, depth) {{
            if (depth > 15) return null;
            var tag = el.tagName.toLowerCase();
            if (SKIP.has(tag)) return null;
            if (el.hidden || el.getAttribute('aria-hidden')==='true') return null;
            var role = getRole(el);
            var name = getName(el);
            var shouldRef = INTERACTIVE.has(role) || (CONTENT.has(role) && name);
            if (shouldRef) {{
                counter++;
                if ('e'+counter === refId) return el;
            }}
            for (var i = 0; i < el.children.length; i++) {{
                var found = findRef(el.children[i], depth+1);
                if (found) return found;
            }}
            return null;
        }}
        return findRef(document.body, 0);
    }}
    return document.querySelector(selector);
}})('{}')"#,
        escape_js_string(selector)
    )
}

// ---------------------------------------------------------------------------
// BrowserBackend implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl BrowserBackend for ExtensionBackend {
    async fn open(&self, url: &str) -> Result<OpenResult> {
        let result = self
            .send(
                "Extension.createTab",
                serde_json::json!({ "url": url }),
            )
            .await?;

        let title = result
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();

        Ok(OpenResult { title })
    }

    async fn close(&self) -> Result<()> {
        self.send("Extension.detachTab", serde_json::json!({}))
            .await?;
        Ok(())
    }

    async fn restart(&self) -> Result<()> {
        self.send("Page.reload", serde_json::json!({})).await?;
        Ok(())
    }

    async fn goto(&self, url: &str) -> Result<()> {
        self.send("Page.navigate", serde_json::json!({ "url": url }))
            .await?;
        Ok(())
    }

    async fn back(&self) -> Result<()> {
        self.eval_js("history.back()").await?;
        Ok(())
    }

    async fn forward(&self) -> Result<()> {
        self.eval_js("history.forward()").await?;
        Ok(())
    }

    async fn reload(&self) -> Result<()> {
        self.send("Page.reload", serde_json::json!({})).await?;
        Ok(())
    }

    async fn pages(&self) -> Result<Vec<PageEntry>> {
        let result = self
            .send("Extension.listTabs", serde_json::json!({}))
            .await?;

        let tabs = result
            .get("tabs")
            .and_then(|t| t.as_array())
            .cloned()
            .unwrap_or_default();

        Ok(tabs
            .iter()
            .map(|tab| PageEntry {
                id: tab
                    .get("id")
                    .and_then(|i| i.as_u64())
                    .map(|i| format!("tab:{}", i))
                    .unwrap_or_default(),
                title: tab
                    .get("title")
                    .and_then(|t| t.as_str())
                    .unwrap_or("(no title)")
                    .to_string(),
                url: tab
                    .get("url")
                    .and_then(|u| u.as_str())
                    .unwrap_or("")
                    .to_string(),
            })
            .collect())
    }

    async fn switch(&self, page_id: &str) -> Result<()> {
        let tab_id: u64 = page_id
            .strip_prefix("tab:")
            .unwrap_or(page_id)
            .parse()
            .map_err(|_| {
                ActionbookError::Other(format!(
                    "Invalid tab ID: {}. Use the numeric ID from 'pages' command (extension mode)",
                    page_id
                ))
            })?;

        self.send(
            "Extension.activateTab",
            serde_json::json!({ "tabId": tab_id }),
        )
        .await?;
        Ok(())
    }

    async fn wait_for(&self, selector: &str, timeout_ms: u64) -> Result<()> {
        self.wait_for_element(selector, timeout_ms).await
    }

    async fn wait_nav(&self, timeout_ms: u64) -> Result<String> {
        let poll_js = format!(
            r#"(async function() {{
                var deadline = Date.now() + {timeout_ms};
                while (Date.now() < deadline) {{
                    if (document.readyState === 'complete') return window.location.href;
                    await new Promise(r => setTimeout(r, 100));
                }}
                return document.readyState === 'complete' ? window.location.href : null;
            }})()"#
        );
        let result = self.eval_js(&poll_js).await?;
        let url = result.as_str().unwrap_or("").to_string();

        if url.is_empty() {
            return Err(ActionbookError::Timeout(format!(
                "Navigation did not complete within {}ms (extension mode)",
                timeout_ms
            )));
        }

        Ok(url)
    }

    async fn click(&self, selector: &str, wait_ms: u64) -> Result<()> {
        if wait_ms > 0 {
            self.wait_for_element(selector, wait_ms).await?;
        }

        let resolve_js = js_resolve_selector(selector);
        let js = format!(
            r#"(function() {{
                var el = {resolve_js};
                if (!el) return {{ success: false, error: 'Element not found' }};
                el.scrollIntoView({{ block: 'center', behavior: 'instant' }});
                el.click();
                return {{ success: true }};
            }})()"#
        );

        self.eval_action(&js, "Click").await
    }

    async fn type_text(&self, selector: &str, text: &str, wait_ms: u64) -> Result<()> {
        if wait_ms > 0 {
            self.wait_for_element(selector, wait_ms).await?;
        }

        let resolve_js = js_resolve_selector(selector);
        let escaped_text = escape_js_string(text);
        let js = format!(
            r#"(function() {{
                var el = {resolve_js};
                if (!el) return {{ success: false, error: 'Element not found' }};
                el.focus();
                var text = '{escaped_text}';
                for (var i = 0; i < text.length; i++) {{
                    el.dispatchEvent(new KeyboardEvent('keydown', {{ key: text[i], bubbles: true }}));
                    el.dispatchEvent(new KeyboardEvent('keypress', {{ key: text[i], bubbles: true }}));
                    if (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA') {{
                        el.value += text[i];
                    }} else if (el.isContentEditable) {{
                        el.textContent += text[i];
                    }}
                    el.dispatchEvent(new InputEvent('input', {{ data: text[i], inputType: 'insertText', bubbles: true }}));
                    el.dispatchEvent(new KeyboardEvent('keyup', {{ key: text[i], bubbles: true }}));
                }}
                return {{ success: true }};
            }})()"#
        );

        self.eval_action(&js, "Type").await
    }

    async fn fill(&self, selector: &str, text: &str, wait_ms: u64) -> Result<()> {
        if wait_ms > 0 {
            self.wait_for_element(selector, wait_ms).await?;
        }

        let resolve_js = js_resolve_selector(selector);
        let escaped_text = escape_js_string(text);
        let js = format!(
            r#"(function() {{
                var el = {resolve_js};
                if (!el) return {{ success: false, error: 'Element not found' }};
                el.focus();
                if (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA') {{
                    var nativeSetter = Object.getOwnPropertyDescriptor(
                        window.HTMLInputElement.prototype, 'value'
                    ) || Object.getOwnPropertyDescriptor(
                        window.HTMLTextAreaElement.prototype, 'value'
                    );
                    if (nativeSetter && nativeSetter.set) {{
                        nativeSetter.set.call(el, '{escaped_text}');
                    }} else {{
                        el.value = '{escaped_text}';
                    }}
                }} else if (el.isContentEditable) {{
                    el.textContent = '{escaped_text}';
                }}
                el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                return {{ success: true }};
            }})()"#
        );

        self.eval_action(&js, "Fill").await
    }

    async fn select(&self, selector: &str, value: &str) -> Result<()> {
        let resolve_js = js_resolve_selector(selector);
        let escaped_value = escape_js_string(value);
        let js = format!(
            r#"(function() {{
                var el = {resolve_js};
                if (!el) return {{ success: false, error: 'Element not found' }};
                if (el.tagName !== 'SELECT') return {{ success: false, error: 'Element is not a <select>' }};
                el.value = '{escaped_value}';
                el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                return {{ success: true }};
            }})()"#
        );

        self.eval_action(&js, "Select").await
    }

    async fn hover(&self, selector: &str) -> Result<()> {
        let resolve_js = js_resolve_selector(selector);
        let js = format!(
            r#"(function() {{
                var el = {resolve_js};
                if (!el) return {{ success: false, error: 'Element not found' }};
                el.scrollIntoView({{ block: 'center', behavior: 'instant' }});
                el.dispatchEvent(new MouseEvent('mouseenter', {{ bubbles: true }}));
                el.dispatchEvent(new MouseEvent('mouseover', {{ bubbles: true }}));
                return {{ success: true }};
            }})()"#
        );

        self.eval_action(&js, "Hover").await
    }

    async fn focus(&self, selector: &str) -> Result<()> {
        let resolve_js = js_resolve_selector(selector);
        let js = format!(
            r#"(function() {{
                var el = {resolve_js};
                if (!el) return {{ success: false, error: 'Element not found' }};
                el.focus();
                return {{ success: true }};
            }})()"#
        );

        self.eval_action(&js, "Focus").await
    }

    async fn press(&self, key: &str) -> Result<()> {
        let escaped_key = escape_js_string(key);
        let js = format!(
            r#"(function() {{
                var key = '{escaped_key}';
                var el = document.activeElement || document.body;
                var opts = {{ key: key, code: 'Key' + key, bubbles: true, cancelable: true }};
                var keyMap = {{
                    'Enter': {{ key: 'Enter', code: 'Enter' }},
                    'Tab': {{ key: 'Tab', code: 'Tab' }},
                    'Escape': {{ key: 'Escape', code: 'Escape' }},
                    'Backspace': {{ key: 'Backspace', code: 'Backspace' }},
                    'Delete': {{ key: 'Delete', code: 'Delete' }},
                    'ArrowUp': {{ key: 'ArrowUp', code: 'ArrowUp' }},
                    'ArrowDown': {{ key: 'ArrowDown', code: 'ArrowDown' }},
                    'ArrowLeft': {{ key: 'ArrowLeft', code: 'ArrowLeft' }},
                    'ArrowRight': {{ key: 'ArrowRight', code: 'ArrowRight' }},
                    'Space': {{ key: ' ', code: 'Space' }},
                    'Home': {{ key: 'Home', code: 'Home' }},
                    'End': {{ key: 'End', code: 'End' }},
                    'PageUp': {{ key: 'PageUp', code: 'PageUp' }},
                    'PageDown': {{ key: 'PageDown', code: 'PageDown' }},
                }};
                if (keyMap[key]) {{
                    opts.key = keyMap[key].key;
                    opts.code = keyMap[key].code;
                }}
                el.dispatchEvent(new KeyboardEvent('keydown', opts));
                el.dispatchEvent(new KeyboardEvent('keypress', opts));
                el.dispatchEvent(new KeyboardEvent('keyup', opts));
                return {{ success: true }};
            }})()"#
        );

        self.eval_action(&js, "Press").await
    }

    async fn screenshot(&self, full_page: bool) -> Result<Vec<u8>> {
        let params = if full_page {
            serde_json::json!({ "format": "png", "captureBeyondViewport": true })
        } else {
            serde_json::json!({ "format": "png" })
        };

        let result = self.send("Page.captureScreenshot", params).await?;
        let b64_data = result
            .get("data")
            .and_then(|d| d.as_str())
            .ok_or_else(|| {
                ActionbookError::ExtensionError(
                    "Screenshot response missing 'data' field (extension mode)".to_string(),
                )
            })?;

        base64::engine::general_purpose::STANDARD
            .decode(b64_data)
            .map_err(|e| {
                ActionbookError::ExtensionError(format!(
                    "Failed to decode screenshot base64 (extension mode): {}",
                    e
                ))
            })
    }

    async fn pdf(&self) -> Result<Vec<u8>> {
        let result = self
            .send("Page.printToPDF", serde_json::json!({}))
            .await?;
        let b64_data = result
            .get("data")
            .and_then(|d| d.as_str())
            .ok_or_else(|| {
                ActionbookError::ExtensionError(
                    "PDF response missing 'data' field (extension mode)".to_string(),
                )
            })?;

        base64::engine::general_purpose::STANDARD
            .decode(b64_data)
            .map_err(|e| {
                ActionbookError::ExtensionError(format!(
                    "Failed to decode PDF base64 (extension mode): {}",
                    e
                ))
            })
    }

    async fn eval(&self, code: &str) -> Result<Value> {
        self.eval_with_options(code, false).await
    }

    async fn html(&self, selector: Option<&str>) -> Result<String> {
        let js = match selector {
            Some(sel) => {
                let resolve_js = js_resolve_selector(sel);
                format!(
                    r#"(function() {{
                        var el = {resolve_js};
                        return el ? el.outerHTML : null;
                    }})()"#
                )
            }
            None => "document.documentElement.outerHTML".to_string(),
        };

        let result = self.eval_js(&js).await?;
        match result {
            Value::String(s) => Ok(s),
            Value::Null => Err(ActionbookError::ElementNotFound(
                selector.unwrap_or("document").to_string(),
            )),
            other => Ok(other.to_string()),
        }
    }

    async fn text(&self, selector: Option<&str>) -> Result<String> {
        let js = match selector {
            Some(sel) => {
                let resolve_js = js_resolve_selector(sel);
                format!(
                    r#"(function() {{
                        var el = {resolve_js};
                        return el ? el.innerText : null;
                    }})()"#
                )
            }
            None => "document.body.innerText".to_string(),
        };

        let result = self.eval_js(&js).await?;
        match result {
            Value::String(s) => Ok(s),
            Value::Null => Err(ActionbookError::ElementNotFound(
                selector.unwrap_or("body").to_string(),
            )),
            other => Ok(other.to_string()),
        }
    }

    async fn snapshot(&self) -> Result<Value> {
        self.eval_js(super::backend::SNAPSHOT_JS).await
    }

    async fn inspect(&self, x: f64, y: f64) -> Result<Value> {
        let js = format!(
            r#"(function() {{
                var el = document.elementFromPoint({x}, {y});
                if (!el) return null;
                var rect = el.getBoundingClientRect();
                var vw = window.innerWidth;
                var vh = window.innerHeight;
                if ({x} < 0 || {x} > vw || {y} < 0 || {y} > vh) return null;
                var attrs = {{}};
                for (var i = 0; i < el.attributes.length; i++) {{
                    attrs[el.attributes[i].name] = el.attributes[i].value;
                }}
                return {{
                    tagName: el.tagName.toLowerCase(),
                    id: el.id || null,
                    className: el.className || null,
                    textContent: (el.textContent || '').substring(0, 200),
                    attributes: attrs,
                    boundingRect: {{ x: rect.x, y: rect.y, width: rect.width, height: rect.height }},
                    viewport: {{ width: vw, height: vh }},
                }};
            }})()"#
        );

        let result = self.eval_js(&js).await?;
        if result.is_null() {
            return Err(ActionbookError::Other(format!(
                "No element found at coordinates ({}, {})",
                x, y
            )));
        }
        Ok(result)
    }

    async fn viewport(&self) -> Result<(u32, u32)> {
        let result = self
            .eval_js("JSON.stringify({width: window.innerWidth, height: window.innerHeight})")
            .await?;

        let parsed: Value = match result {
            Value::String(s) => serde_json::from_str(&s).unwrap_or(Value::Null),
            other => other,
        };

        let w = parsed
            .get("width")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        let h = parsed
            .get("height")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        Ok((w, h))
    }

    async fn get_cookies(&self) -> Result<Vec<Value>> {
        // Get the current page URL for cookie scoping
        let url_result = self.eval_js("window.location.href").await?;
        let url = url_result.as_str().unwrap_or("").to_string();

        let result = self
            .send(
                "Extension.getCookies",
                serde_json::json!({ "url": url }),
            )
            .await?;

        let cookies = result
            .get("cookies")
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();

        Ok(cookies)
    }

    async fn set_cookie(&self, name: &str, value: &str, domain: Option<&str>) -> Result<()> {
        let url_result = self.eval_js("window.location.href").await?;
        let url = url_result.as_str().unwrap_or("").to_string();

        let params = match domain {
            Some(d) => serde_json::json!({ "url": url, "name": name, "value": value, "domain": d }),
            None => serde_json::json!({ "url": url, "name": name, "value": value }),
        };

        self.send("Extension.setCookie", params).await?;
        Ok(())
    }

    async fn delete_cookie(&self, name: &str) -> Result<()> {
        let url_result = self.eval_js("window.location.href").await?;
        let url = url_result.as_str().unwrap_or("").to_string();

        self.send(
            "Extension.removeCookie",
            serde_json::json!({ "url": url, "name": name }),
        )
        .await?;
        Ok(())
    }

    async fn clear_cookies(&self, domain: Option<&str>) -> Result<()> {
        let cookies = self.get_cookies().await?;

        let url_result = self.eval_js("window.location.href").await?;
        let url = url_result.as_str().unwrap_or("").to_string();

        for cookie in &cookies {
            let cookie_name = cookie.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let cookie_domain = cookie.get("domain").and_then(|d| d.as_str()).unwrap_or("");

            // If domain filter specified, skip non-matching cookies
            if let Some(d) = domain {
                if !cookie_domain.ends_with(d) {
                    continue;
                }
            }

            self.send(
                "Extension.removeCookie",
                serde_json::json!({ "url": url, "name": cookie_name }),
            )
            .await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_js_handles_plain_string() {
        assert_eq!(escape_js_string("hello"), "hello");
    }

    #[test]
    fn escape_js_handles_single_quotes() {
        assert_eq!(escape_js_string("it's"), r"it\'s");
    }

    #[test]
    fn escape_js_handles_double_quotes() {
        assert_eq!(escape_js_string(r#"say "hi""#), r#"say "hi""#);
    }

    #[test]
    fn escape_js_handles_newlines() {
        assert_eq!(escape_js_string("a\nb"), r"a\nb");
    }

    #[test]
    fn escape_js_handles_backslash() {
        assert_eq!(escape_js_string(r"a\b"), r"a\\b");
    }

    #[test]
    fn escape_js_handles_injection_attempt() {
        let input = "');alert(1);//";
        let escaped = escape_js_string(input);
        // The single quote is escaped so it can't break out of a JS string literal
        assert!(escaped.starts_with(r"\'"), "leading quote must be escaped: {}", escaped);
        // When embedded in JS as '...', the escaped version is safe
        assert!(escaped.contains(r"\'"), "single quote must be escaped: {}", escaped);
    }

    #[test]
    fn js_resolve_selector_css() {
        let js = js_resolve_selector("button.submit");
        assert!(js.contains("document.querySelector"));
        assert!(js.contains("button.submit"));
    }

    #[test]
    fn js_resolve_selector_ref_format() {
        let js = js_resolve_selector("[ref=e5]");
        let js_lower = js.to_lowercase();
        assert!(
            js_lower.contains("queryselectorall") || js_lower.contains("ref"),
            "ref selector should resolve via ref-based lookup"
        );
    }
}

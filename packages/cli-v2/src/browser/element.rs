//! Shared element resolution utilities.
//!
//! [`TabContext`] bundles the per-tab CDP session, registry, and IDs so that
//! every command can resolve selectors with a single `ctx.resolve_node(sel)`.
//!
//! Selector dispatch:
//! 1. **CSS selector** — default path, `DOM.querySelector`.
//! 2. **XPath** — prefix `//` or `/`, `Runtime.evaluate`.
//! 3. **Snapshot ref** — prefix `@e`, e.g. `@e5`, via RefCache + CDP.
//!
//! iframe support: after resolving an `@eN` ref, `resolved_frame_id` is set
//! so that subsequent `execute_on_element()` calls route to the correct CDP
//! session for cross-origin iframes.

use serde_json::{Value, json};

use crate::action_result::ActionResult;
use crate::daemon::cdp_session::{CdpSession, cdp_error_to_result, get_cdp_and_target};
use crate::daemon::registry::SharedRegistry;
use crate::error::CliError;

// ── Frame-aware CDP execution ─────────────────────────────────────

/// Execute a CDP command on the correct session for a given frame_id.
///
/// - Cross-origin iframes (found in `cdp.iframe_sessions()`): use their dedicated session.
/// - Same-origin iframes and main frame: use `execute_on_tab` (parent session).
pub async fn execute_for_frame(
    cdp: &CdpSession,
    target_id: &str,
    frame_id: Option<&str>,
    method: &str,
    params: Value,
) -> Result<Value, CliError> {
    if let Some(fid) = frame_id {
        let iframe_sessions = cdp.iframe_sessions().await;
        if let Some(iframe_sid) = iframe_sessions.get(fid) {
            return cdp.execute(method, params, Some(iframe_sid)).await;
        }
    }
    // Main frame or same-origin iframe: use parent tab session
    cdp.execute_on_tab(target_id, method, params).await
}

// ── TabContext ─────────────────────────────────────────────────────

/// Per-tab context bundle for element resolution.
///
/// Created once per command execution via [`TabContext::new`].
/// Exposes `cdp` and `target_id` as pub fields so callers can also
/// issue non-element CDP calls (e.g. `Input.dispatchMouseEvent`).
///
/// After resolving a selector, `resolved_frame_id` tracks the frame
/// the element belongs to. Use `execute_on_element()` for subsequent
/// DOM/Runtime commands on that element.
pub struct TabContext {
    pub cdp: CdpSession,
    pub target_id: String,
    registry: SharedRegistry,
    session_id: String,
    tab_id: String,
    /// Frame context set by the most recent resolve_node / resolve_center / resolve_object call.
    /// None = main frame. Some(frame_id) = iframe element.
    /// Used by execute_on_element() for subsequent CDP commands on the same element.
    resolved_frame_id: Option<String>,
}

impl TabContext {
    /// Build a context by looking up the CDP session and target for a tab.
    pub async fn new(
        registry: &SharedRegistry,
        session_id: &str,
        tab_id: &str,
    ) -> Result<Self, ActionResult> {
        let (cdp, target_id) = get_cdp_and_target(registry, session_id, tab_id).await?;
        Ok(Self {
            cdp,
            target_id,
            registry: registry.clone(),
            session_id: session_id.to_string(),
            tab_id: tab_id.to_string(),
            resolved_frame_id: None,
        })
    }

    /// Selector → CDP `nodeId`. Sets `resolved_frame_id` for @eN refs.
    pub async fn resolve_node(&mut self, selector: &str) -> Result<i64, ActionResult> {
        if selector.starts_with("@e") {
            let (node_id, frame_id) = resolve_ref(
                &self.cdp,
                &self.target_id,
                selector,
                &self.registry,
                &self.session_id,
                &self.tab_id,
            )
            .await?;
            self.resolved_frame_id = frame_id;
            Ok(node_id)
        } else if selector.starts_with("//") || selector.starts_with('/') {
            self.resolved_frame_id = None;
            resolve_xpath(&self.cdp, &self.target_id, selector).await
        } else {
            self.resolved_frame_id = None;
            resolve_css(&self.cdp, &self.target_id, selector).await
        }
    }

    /// Selector → centre `(x, y)` coordinates. Sets `resolved_frame_id`.
    pub async fn resolve_center(&mut self, selector: &str) -> Result<(f64, f64), ActionResult> {
        let node_id = self.resolve_node(selector).await?;
        let frame_id = self.resolved_frame_id.as_deref();
        scroll_into_view_for_frame(&self.cdp, &self.target_id, node_id, frame_id).await?;
        get_element_center_for_frame(&self.cdp, &self.target_id, node_id, selector, frame_id).await
    }

    /// Selector → `(nodeId, objectId)`. Sets `resolved_frame_id`.
    pub async fn resolve_object(&mut self, selector: &str) -> Result<(i64, String), ActionResult> {
        let node_id = self.resolve_node(selector).await?;
        let frame_id = self.resolved_frame_id.as_deref();
        let object_id =
            resolve_object_id_for_frame(&self.cdp, &self.target_id, node_id, frame_id).await?;
        Ok((node_id, object_id))
    }

    /// Scroll an element into the viewport if needed.
    pub async fn scroll_into_view(&self, node_id: i64) -> Result<(), ActionResult> {
        let frame_id = self.resolved_frame_id.as_deref();
        scroll_into_view_for_frame(&self.cdp, &self.target_id, node_id, frame_id).await
    }

    /// `nodeId` → remote JS object ID for `Runtime.callFunctionOn`.
    pub async fn resolve_object_id(&self, node_id: i64) -> Result<String, ActionResult> {
        let frame_id = self.resolved_frame_id.as_deref();
        resolve_object_id_for_frame(&self.cdp, &self.target_id, node_id, frame_id).await
    }

    /// Execute a CDP command on the frame of the most recently resolved element.
    ///
    /// Use for: DOM.focus, Runtime.callFunctionOn, Runtime.evaluate (on element),
    /// DOM.setFileInputFiles, etc. Falls back to execute_on_tab if no frame context.
    pub async fn execute_on_element(&self, method: &str, params: Value) -> Result<Value, CliError> {
        execute_for_frame(
            &self.cdp,
            &self.target_id,
            self.resolved_frame_id.as_deref(),
            method,
            params,
        )
        .await
    }

    /// Access the resolved frame_id (set by the most recent resolve call).
    pub fn resolved_frame_id(&self) -> Option<&str> {
        self.resolved_frame_id.as_deref()
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn tab_id(&self) -> &str {
        &self.tab_id
    }

    pub fn registry(&self) -> &SharedRegistry {
        &self.registry
    }
}

// ── Standalone helpers (frame-aware versions) ─────────────────────

/// Scroll an element into the viewport, routing to the correct frame session.
async fn scroll_into_view_for_frame(
    cdp: &CdpSession,
    target_id: &str,
    node_id: i64,
    frame_id: Option<&str>,
) -> Result<(), ActionResult> {
    execute_for_frame(
        cdp,
        target_id,
        frame_id,
        "DOM.scrollIntoViewIfNeeded",
        json!({ "nodeId": node_id }),
    )
    .await
    .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;
    Ok(())
}

/// `nodeId` → remote JS object ID, routing to the correct frame session.
async fn resolve_object_id_for_frame(
    cdp: &CdpSession,
    target_id: &str,
    node_id: i64,
    frame_id: Option<&str>,
) -> Result<String, ActionResult> {
    let resolve_resp = execute_for_frame(
        cdp,
        target_id,
        frame_id,
        "DOM.resolveNode",
        json!({ "nodeId": node_id }),
    )
    .await
    .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;

    resolve_resp
        .pointer("/result/object/objectId")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| ActionResult::fatal("CDP_ERROR", "could not resolve element to JS object"))
}

/// Get the centre point of an element's bounding box, routing to the correct frame session.
async fn get_element_center_for_frame(
    cdp: &CdpSession,
    target_id: &str,
    node_id: i64,
    selector: &str,
    frame_id: Option<&str>,
) -> Result<(f64, f64), ActionResult> {
    let bm = execute_for_frame(
        cdp,
        target_id,
        frame_id,
        "DOM.getBoxModel",
        json!({ "nodeId": node_id }),
    )
    .await
    .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;

    let content = bm
        .pointer("/result/model/content")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            ActionResult::fatal("CDP_ERROR", format!("no box model for element: {selector}"))
        })?;

    let cx = (content[0].as_f64().unwrap_or(0.0) + content[4].as_f64().unwrap_or(0.0)) / 2.0;
    let cy = (content[1].as_f64().unwrap_or(0.0) + content[5].as_f64().unwrap_or(0.0)) / 2.0;
    Ok((cx, cy))
}

// ── Legacy standalone helpers (main-frame only, kept for non-TabContext callers) ──

/// Scroll an element into the viewport if it is not already visible.
pub async fn scroll_into_view(
    cdp: &CdpSession,
    target_id: &str,
    node_id: i64,
) -> Result<(), ActionResult> {
    scroll_into_view_for_frame(cdp, target_id, node_id, None).await
}

/// `nodeId` → remote JS object ID.
pub async fn resolve_object_id(
    cdp: &CdpSession,
    target_id: &str,
    node_id: i64,
) -> Result<String, ActionResult> {
    resolve_object_id_for_frame(cdp, target_id, node_id, None).await
}

/// Get the centre point of an element's bounding box.
pub async fn get_element_center(
    cdp: &CdpSession,
    target_id: &str,
    node_id: i64,
    selector: &str,
) -> Result<(f64, f64), ActionResult> {
    get_element_center_for_frame(cdp, target_id, node_id, selector, None).await
}

pub fn element_not_found(selector: &str) -> ActionResult {
    // Detect likely snapshot ref without the @ prefix (e.g. "e46" instead of "@e46")
    let hint = if selector.starts_with('e')
        && selector.len() > 1
        && selector[1..].chars().all(|c| c.is_ascii_digit())
    {
        format!("did you mean @{selector}? snapshot refs require the @ prefix")
    } else {
        String::new()
    };

    ActionResult::Fatal {
        code: "ELEMENT_NOT_FOUND".to_string(),
        message: format!("element not found: {selector}"),
        hint,
        details: Some(json!({ "selector": selector })),
    }
}

// ── Private resolvers ──────────────────────────────────────────────

/// CSS selector → nodeId via `DOM.querySelector`.
async fn resolve_css(
    cdp: &CdpSession,
    target_id: &str,
    selector: &str,
) -> Result<i64, ActionResult> {
    let doc = cdp
        .execute_on_tab(target_id, "DOM.getDocument", json!({}))
        .await
        .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;

    let root_id = doc
        .pointer("/result/root/nodeId")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let query = match cdp
        .execute_on_tab(
            target_id,
            "DOM.querySelector",
            json!({ "nodeId": root_id, "selector": selector }),
        )
        .await
    {
        Ok(v) => v,
        Err(e) => {
            let msg = e.to_string();
            // CDP -32000 "DOM Error while querying" means invalid CSS selector syntax
            if msg.contains("DOM Error while querying") {
                return Err(ActionResult::Fatal {
                    code: "INVALID_SELECTOR".to_string(),
                    message: format!("invalid CSS selector: '{selector}'"),
                    hint: if selector.starts_with('@') {
                        "snapshot refs must use @eN format (e.g. @e5), not @N".to_string()
                    } else {
                        "check your selector syntax".to_string()
                    },
                    details: Some(json!({ "selector": selector })),
                });
            }
            return Err(cdp_error_to_result(e, "CDP_ERROR"));
        }
    };

    let node_id = query
        .pointer("/result/nodeId")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    if node_id == 0 {
        return Err(element_not_found(selector));
    }
    Ok(node_id)
}

/// XPath expression → nodeId via `Runtime.evaluate` + `DOM.requestNode`.
async fn resolve_xpath(
    cdp: &CdpSession,
    target_id: &str,
    selector: &str,
) -> Result<i64, ActionResult> {
    cdp.execute_on_tab(target_id, "DOM.getDocument", json!({}))
        .await
        .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;

    let xpath_json = serde_json::to_string(selector).unwrap_or_default();
    let js = format!(
        r#"(() => {{
            const r = document.evaluate({xpath_json}, document, null, XPathResult.FIRST_ORDERED_NODE_TYPE, null);
            return r.singleNodeValue;
        }})()"#
    );

    let eval = cdp
        .execute_on_tab(target_id, "Runtime.evaluate", json!({ "expression": js }))
        .await
        .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;

    let object_id = match eval
        .pointer("/result/result/objectId")
        .and_then(|v| v.as_str())
    {
        Some(id) => id.to_string(),
        None => return Err(element_not_found(selector)),
    };

    let node_resp = cdp
        .execute_on_tab(
            target_id,
            "DOM.requestNode",
            json!({ "objectId": object_id }),
        )
        .await
        .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;

    let node_id = node_resp
        .pointer("/result/nodeId")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    if node_id == 0 {
        return Err(element_not_found(selector));
    }
    Ok(node_id)
}

/// Snapshot ref (`@eN`) → (nodeId, frame_id) via RefCache + CDP.
///
/// Returns both the resolved nodeId and the frame_id from RefCache,
/// so callers can route subsequent CDP commands to the correct session.
///
/// Strategy: try backendNodeId directly (> 0), then fall back to
/// `Accessibility.queryAXTree` with role + name.
#[allow(clippy::too_many_arguments)]
async fn resolve_ref(
    cdp: &CdpSession,
    target_id: &str,
    selector: &str,
    registry: &SharedRegistry,
    session_id: &str,
    tab_id: &str,
) -> Result<(i64, Option<String>), ActionResult> {
    let ref_id = selector.strip_prefix('@').unwrap_or(selector);

    if !ref_id.starts_with('e') || ref_id.len() < 2 || ref_id[1..].parse::<u64>().is_err() {
        return Err(ActionResult::fatal(
            "INVALID_ARGUMENT",
            format!("invalid snapshot ref format: '{selector}' (expected @eN)"),
        ));
    }

    let (backend_node_id, role, name, frame_id) = {
        let reg = registry.lock().await;
        let cache = reg.peek_ref_cache(session_id, tab_id);
        let bid = cache.and_then(|c| c.backend_node_id_for_ref(ref_id));
        let entry = cache.and_then(|c| c.entry_for_ref(ref_id));
        let fid = cache
            .and_then(|c| c.frame_id_for_ref(ref_id))
            .map(String::from);
        (
            bid,
            entry.map(|e| e.role.clone()).unwrap_or_default(),
            entry.map(|e| e.name.clone()).unwrap_or_default(),
            fid,
        )
    };

    let backend_node_id = backend_node_id.ok_or_else(|| {
        ActionResult::fatal_with_hint(
            "REF_NOT_FOUND",
            format!("snapshot ref '{selector}' not found"),
            "run 'browser snapshot' first to generate element refs",
        )
    })?;

    // Get document on the correct frame session
    execute_for_frame(
        cdp,
        target_id,
        frame_id.as_deref(),
        "DOM.getDocument",
        json!({}),
    )
    .await
    .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;

    // Try direct resolution for real backendNodeIds (> 0)
    if backend_node_id > 0
        && let Some(node_id) =
            resolve_backend_node(cdp, target_id, backend_node_id, frame_id.as_deref()).await?
    {
        return Ok((node_id, frame_id));
    }

    // Fallback: use role + name via Accessibility.queryAXTree
    if !name.is_empty()
        && let Some(node_id) =
            resolve_by_ax_query(cdp, target_id, &role, &name, frame_id.as_deref()).await?
    {
        return Ok((node_id, frame_id));
    }

    Err(ActionResult::fatal_with_hint(
        "REF_STALE",
        format!("snapshot ref '{selector}' could not be resolved (role={role}, name={name})"),
        "run 'browser snapshot' again",
    ))
}

/// backendNodeId → nodeId. Returns `Ok(None)` if stale (-32000).
/// Routes to the correct frame session for cross-origin iframes.
async fn resolve_backend_node(
    cdp: &CdpSession,
    target_id: &str,
    backend_node_id: i64,
    frame_id: Option<&str>,
) -> Result<Option<i64>, ActionResult> {
    let resolve_resp = match execute_for_frame(
        cdp,
        target_id,
        frame_id,
        "DOM.resolveNode",
        json!({ "backendNodeId": backend_node_id }),
    )
    .await
    {
        Ok(resp) => resp,
        Err(e) => {
            let err_str = format!("{e:?}");
            if err_str.contains("-32000") {
                return Ok(None);
            }
            return Err(cdp_error_to_result(e, "CDP_ERROR"));
        }
    };

    let object_id = match resolve_resp
        .pointer("/result/object/objectId")
        .and_then(|v| v.as_str())
    {
        Some(id) => id.to_string(),
        None => return Ok(None),
    };

    let node_resp = execute_for_frame(
        cdp,
        target_id,
        frame_id,
        "DOM.requestNode",
        json!({ "objectId": object_id }),
    )
    .await
    .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;

    let node_id = node_resp
        .pointer("/result/nodeId")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    if node_id > 0 {
        Ok(Some(node_id))
    } else {
        Ok(None)
    }
}

/// Find an element by ARIA role + name via `Accessibility.queryAXTree`.
/// Routes to the correct frame session for cross-origin iframes.
async fn resolve_by_ax_query(
    cdp: &CdpSession,
    target_id: &str,
    role: &str,
    name: &str,
    frame_id: Option<&str>,
) -> Result<Option<i64>, ActionResult> {
    let resp = match execute_for_frame(
        cdp,
        target_id,
        frame_id,
        "Accessibility.queryAXTree",
        json!({ "accessibleName": name, "role": role }),
    )
    .await
    {
        Ok(r) => r,
        Err(_) => return Ok(None),
    };

    let nodes = resp.pointer("/result/nodes").and_then(|v| v.as_array());
    let nodes = match nodes {
        Some(n) if !n.is_empty() => n,
        _ => return Ok(None),
    };

    for node in nodes {
        let bid = node["backendDOMNodeId"].as_i64().unwrap_or(0);
        if bid > 0
            && let Some(node_id) = resolve_backend_node(cdp, target_id, bid, frame_id).await?
        {
            return Ok(Some(node_id));
        }
    }

    Ok(None)
}

// ── Target parsing (shared by click, fill, type) ──────────────────

/// Result of parsing a positional target argument.
pub enum ClickTarget {
    Coordinates(f64, f64),
    Selector(String),
}

/// Parse a target string into coordinates or a CSS selector.
///
/// Heuristic: if the first character is a digit, comma, or minus-digit,
/// treat it as a coordinate attempt and validate strictly. Otherwise it
/// is a CSS selector.
pub fn parse_target(input: &str) -> Result<ClickTarget, ActionResult> {
    let trimmed = input.trim();
    let first = trimmed.chars().next().unwrap_or(' ');

    let is_coord_attempt = first.is_ascii_digit()
        || first == ','
        || (first == '-' && trimmed.chars().nth(1).is_some_and(|c| c.is_ascii_digit()));

    if !is_coord_attempt {
        return Ok(ClickTarget::Selector(trimmed.to_string()));
    }

    let parts: Vec<&str> = trimmed.splitn(2, ',').collect();
    if parts.len() != 2 {
        return Err(ActionResult::fatal(
            "INVALID_ARGUMENT",
            format!("invalid coordinates: '{input}'"),
        ));
    }

    let x = parts[0].trim().parse::<f64>().map_err(|_| {
        ActionResult::fatal(
            "INVALID_ARGUMENT",
            format!("invalid coordinates: '{input}'"),
        )
    })?;
    let y = parts[1].trim().parse::<f64>().map_err(|_| {
        ActionResult::fatal(
            "INVALID_ARGUMENT",
            format!("invalid coordinates: '{input}'"),
        )
    })?;

    Ok(ClickTarget::Coordinates(x, y))
}

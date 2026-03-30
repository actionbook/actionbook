//! Shared element resolution utilities.
//!
//! [`TabContext`] bundles the per-tab CDP session, registry, and IDs so that
//! every command can resolve selectors with a single `ctx.resolve_node(sel)`.
//!
//! Selector dispatch:
//! 1. **CSS selector** — default path, `DOM.querySelector`.
//! 2. **XPath** — prefix `//` or `/`, `Runtime.evaluate`.
//! 3. **Snapshot ref** — prefix `@e`, e.g. `@e5`, via RefCache + CDP.

use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::cdp_session::{CdpSession, cdp_error_to_result, get_cdp_and_target};
use crate::daemon::registry::SharedRegistry;

// ── TabContext ─────────────────────────────────────────────────────

/// Per-tab context bundle for element resolution.
///
/// Created once per command execution via [`TabContext::new`].
/// Exposes `cdp` and `target_id` as pub fields so callers can also
/// issue non-element CDP calls (e.g. `Input.dispatchMouseEvent`).
pub struct TabContext {
    pub cdp: CdpSession,
    pub target_id: String,
    registry: SharedRegistry,
    session_id: String,
    tab_id: String,
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
        })
    }

    /// Selector → CDP `nodeId`.
    pub async fn resolve_node(&self, selector: &str) -> Result<i64, ActionResult> {
        if selector.starts_with("@e") {
            resolve_ref(
                &self.cdp,
                &self.target_id,
                selector,
                &self.registry,
                &self.session_id,
                &self.tab_id,
            )
            .await
        } else if selector.starts_with("//") || selector.starts_with('/') {
            resolve_xpath(&self.cdp, &self.target_id, selector).await
        } else {
            resolve_css(&self.cdp, &self.target_id, selector).await
        }
    }

    /// Selector → centre `(x, y)` coordinates.
    pub async fn resolve_center(&self, selector: &str) -> Result<(f64, f64), ActionResult> {
        let node_id = self.resolve_node(selector).await?;
        self.scroll_into_view(node_id).await?;
        get_element_center(&self.cdp, &self.target_id, node_id, selector).await
    }

    /// Selector → `(nodeId, objectId)`.
    pub async fn resolve_object(&self, selector: &str) -> Result<(i64, String), ActionResult> {
        let node_id = self.resolve_node(selector).await?;
        let object_id = self.resolve_object_id(node_id).await?;
        Ok((node_id, object_id))
    }

    /// Scroll an element into the viewport if needed.
    pub async fn scroll_into_view(&self, node_id: i64) -> Result<(), ActionResult> {
        scroll_into_view(&self.cdp, &self.target_id, node_id).await
    }

    /// `nodeId` → remote JS object ID for `Runtime.callFunctionOn`.
    pub async fn resolve_object_id(&self, node_id: i64) -> Result<String, ActionResult> {
        resolve_object_id(&self.cdp, &self.target_id, node_id).await
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

// ── Standalone helpers (used by TabContext + low-level callers) ────

/// Scroll an element into the viewport if it is not already visible.
pub async fn scroll_into_view(
    cdp: &CdpSession,
    target_id: &str,
    node_id: i64,
) -> Result<(), ActionResult> {
    cdp.execute_on_tab(
        target_id,
        "DOM.scrollIntoViewIfNeeded",
        json!({ "nodeId": node_id }),
    )
    .await
    .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;
    Ok(())
}

/// `nodeId` → remote JS object ID.
pub async fn resolve_object_id(
    cdp: &CdpSession,
    target_id: &str,
    node_id: i64,
) -> Result<String, ActionResult> {
    let resolve_resp = cdp
        .execute_on_tab(target_id, "DOM.resolveNode", json!({ "nodeId": node_id }))
        .await
        .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;

    resolve_resp
        .pointer("/result/object/objectId")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| ActionResult::fatal("CDP_ERROR", "could not resolve element to JS object"))
}

/// Get the centre point of an element's bounding box.
pub async fn get_element_center(
    cdp: &CdpSession,
    target_id: &str,
    node_id: i64,
    selector: &str,
) -> Result<(f64, f64), ActionResult> {
    let bm = cdp
        .execute_on_tab(target_id, "DOM.getBoxModel", json!({ "nodeId": node_id }))
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

pub fn element_not_found(selector: &str) -> ActionResult {
    ActionResult::Fatal {
        code: "ELEMENT_NOT_FOUND".to_string(),
        message: format!("element not found: {selector}"),
        hint: String::new(),
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

    let query = cdp
        .execute_on_tab(
            target_id,
            "DOM.querySelector",
            json!({ "nodeId": root_id, "selector": selector }),
        )
        .await
        .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;

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

/// Snapshot ref (`@eN`) → nodeId via RefCache + CDP.
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
) -> Result<i64, ActionResult> {
    let ref_id = selector.strip_prefix('@').unwrap_or(selector);

    if !ref_id.starts_with('e') || ref_id.len() < 2 || ref_id[1..].parse::<u64>().is_err() {
        return Err(ActionResult::fatal(
            "INVALID_ARGUMENT",
            format!("invalid snapshot ref format: '{selector}' (expected @eN)"),
        ));
    }

    let (backend_node_id, role, name) = {
        let reg = registry.lock().await;
        let cache = reg.peek_ref_cache(session_id, tab_id);
        let bid = cache.and_then(|c| c.backend_node_id_for_ref(ref_id));
        let entry = cache.and_then(|c| c.entry_for_ref(ref_id));
        (
            bid,
            entry.map(|e| e.role.clone()).unwrap_or_default(),
            entry.map(|e| e.name.clone()).unwrap_or_default(),
        )
    };

    let backend_node_id = backend_node_id.ok_or_else(|| {
        ActionResult::fatal_with_hint(
            "REF_NOT_FOUND",
            format!("snapshot ref '{selector}' not found"),
            "run 'browser snapshot' first to generate element refs",
        )
    })?;

    cdp.execute_on_tab(target_id, "DOM.getDocument", json!({}))
        .await
        .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;

    // Try direct resolution for real backendNodeIds (> 0)
    if backend_node_id > 0
        && let Some(node_id) = resolve_backend_node(cdp, target_id, backend_node_id).await?
    {
        return Ok(node_id);
    }

    // Fallback: use role + name via Accessibility.queryAXTree
    if !name.is_empty()
        && let Some(node_id) = resolve_by_ax_query(cdp, target_id, &role, &name).await?
    {
        return Ok(node_id);
    }

    Err(ActionResult::fatal_with_hint(
        "REF_STALE",
        format!("snapshot ref '{selector}' could not be resolved (role={role}, name={name})"),
        "run 'browser snapshot' again",
    ))
}

/// backendNodeId → nodeId. Returns `Ok(None)` if stale (-32000).
async fn resolve_backend_node(
    cdp: &CdpSession,
    target_id: &str,
    backend_node_id: i64,
) -> Result<Option<i64>, ActionResult> {
    let resolve_resp = match cdp
        .execute_on_tab(
            target_id,
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

    if node_id > 0 {
        Ok(Some(node_id))
    } else {
        Ok(None)
    }
}

/// Find an element by ARIA role + name via `Accessibility.queryAXTree`.
async fn resolve_by_ax_query(
    cdp: &CdpSession,
    target_id: &str,
    role: &str,
    name: &str,
) -> Result<Option<i64>, ActionResult> {
    let resp = match cdp
        .execute_on_tab(
            target_id,
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
            && let Some(node_id) = resolve_backend_node(cdp, target_id, bid).await?
        {
            return Ok(Some(node_id));
        }
    }

    Ok(None)
}

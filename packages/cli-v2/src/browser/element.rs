//! Shared element resolution utilities.
//!
//! Every command that accepts a `<selector>` argument (click, hover, focus,
//! fill, …) delegates to this module so selector semantics are consistent:
//!
//! 1. **CSS selector** — default path, uses `DOM.querySelector`.
//! 2. **XPath** — prefix `//` or `/`, uses `Runtime.evaluate` with
//!    `document.evaluate()`.
//! 3. **Snapshot ref** — prefix `@e`, e.g. `@e5`. Resolves via the
//!    per-tab `RefCache` stored in the daemon registry.

use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::cdp_session::{CdpSession, cdp_error_to_result};
use crate::daemon::registry::SharedRegistry;

/// Resolve a `<selector>` string to a CDP `nodeId`.
///
/// Dispatches by selector form:
///   - `@eN`  → snapshot ref (via RefCache)
///   - `//…`  → XPath
///   - `/…`   → XPath (absolute)
///   - else   → CSS selector
pub async fn resolve_node(
    cdp: &CdpSession,
    target_id: &str,
    selector: &str,
    registry: &SharedRegistry,
    session_id: &str,
    tab_id: &str,
) -> Result<i64, ActionResult> {
    if selector.starts_with("@e") {
        resolve_ref(cdp, target_id, selector, registry, session_id, tab_id).await
    } else if selector.starts_with("//") || selector.starts_with('/') {
        resolve_xpath(cdp, target_id, selector).await
    } else {
        resolve_css(cdp, target_id, selector).await
    }
}

/// Scroll an element into the viewport if it is not already visible.
///
/// Uses `DOM.scrollIntoViewIfNeeded` so off-screen elements become
/// reachable before we compute their bounding-box coordinates.
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

/// Get the centre point of an element's bounding box given its `nodeId`.
///
/// Scrolls the element into view first so that coordinates are always
/// within the visible viewport.
pub async fn get_element_center(
    cdp: &CdpSession,
    target_id: &str,
    node_id: i64,
    selector: &str,
) -> Result<(f64, f64), ActionResult> {
    scroll_into_view(cdp, target_id, node_id).await?;

    let bm = cdp
        .execute_on_tab(target_id, "DOM.getBoxModel", json!({ "nodeId": node_id }))
        .await
        .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;

    // content quad: [x1,y1, x2,y2, x3,y3, x4,y4]
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

/// Convenience: selector string → centre coordinates in one call.
pub async fn resolve_element_center(
    cdp: &CdpSession,
    target_id: &str,
    selector: &str,
    registry: &SharedRegistry,
    session_id: &str,
    tab_id: &str,
) -> Result<(f64, f64), ActionResult> {
    let node_id = resolve_node(cdp, target_id, selector, registry, session_id, tab_id).await?;
    get_element_center(cdp, target_id, node_id, selector).await
}

/// Convert a DOM `nodeId` to a remote JS object ID suitable for
/// `Runtime.callFunctionOn`.
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

/// Convenience: selector string → `(nodeId, objectId)` in one call.
pub async fn resolve_selector_object(
    cdp: &CdpSession,
    target_id: &str,
    selector: &str,
    registry: &SharedRegistry,
    session_id: &str,
    tab_id: &str,
) -> Result<(i64, String), ActionResult> {
    let node_id = resolve_node(cdp, target_id, selector, registry, session_id, tab_id).await?;
    let object_id = resolve_object_id(cdp, target_id, node_id).await?;
    Ok((node_id, object_id))
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
    // Materialize the DOM tree for this target before converting a runtime
    // node handle back into a DOM nodeId. Without this, DOM.requestNode can
    // return nodeId=0 for otherwise valid XPath matches.
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

    // If the result subtype is "null" or has no objectId, element was not found.
    let object_id = eval
        .pointer("/result/result/objectId")
        .and_then(|v| v.as_str());

    let object_id = match object_id {
        Some(id) => id.to_string(),
        None => return Err(element_not_found(selector)),
    };

    // Convert remote object → DOM nodeId
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
async fn resolve_ref(
    cdp: &CdpSession,
    target_id: &str,
    selector: &str,
    registry: &SharedRegistry,
    session_id: &str,
    tab_id: &str,
) -> Result<i64, ActionResult> {
    let ref_id = selector.strip_prefix('@').unwrap_or(selector);

    // Validate format: must be "eN" where N is a positive integer
    if !ref_id.starts_with('e') || ref_id.len() < 2 || ref_id[1..].parse::<u64>().is_err() {
        return Err(ActionResult::fatal(
            "INVALID_ARGUMENT",
            format!("invalid snapshot ref format: '{selector}' (expected @eN)"),
        ));
    }

    // Look up backendNodeId from the tab's RefCache
    let backend_node_id = {
        let reg = registry.lock().await;
        reg.peek_ref_cache(session_id, tab_id)
            .and_then(|cache| cache.backend_node_id_for_ref(ref_id))
    };

    let backend_node_id = backend_node_id.ok_or_else(|| {
        ActionResult::fatal_with_hint(
            "REF_NOT_FOUND",
            format!("snapshot ref '{selector}' not found"),
            "run 'browser snapshot' first to generate element refs",
        )
    })?;

    // Materialize DOM tree
    cdp.execute_on_tab(target_id, "DOM.getDocument", json!({}))
        .await
        .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;

    // backendNodeId → objectId
    let resolve_resp = cdp
        .execute_on_tab(
            target_id,
            "DOM.resolveNode",
            json!({ "backendNodeId": backend_node_id }),
        )
        .await
        .map_err(|_| {
            ActionResult::fatal_with_hint(
                "REF_STALE",
                format!("snapshot ref '{selector}' is stale — element no longer exists in the DOM"),
                "run 'browser snapshot' again",
            )
        })?;

    let object_id = resolve_resp
        .pointer("/result/object/objectId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ActionResult::fatal_with_hint(
                "REF_STALE",
                format!("snapshot ref '{selector}' could not be resolved"),
                "run 'browser snapshot' again",
            )
        })?;

    // objectId → nodeId
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
        return Err(ActionResult::fatal_with_hint(
            "REF_STALE",
            format!("snapshot ref '{selector}' resolved but DOM node is inaccessible"),
            "run 'browser snapshot' again",
        ));
    }

    Ok(node_id)
}

// ── Error helper ───────────────────────────────────────────────────

pub fn element_not_found(selector: &str) -> ActionResult {
    ActionResult::Fatal {
        code: "ELEMENT_NOT_FOUND".to_string(),
        message: format!("element not found: {selector}"),
        hint: String::new(),
        details: Some(json!({ "selector": selector })),
    }
}

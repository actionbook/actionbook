//! Shared element resolution utilities.
//!
//! Every command that accepts a `<selector>` argument (click, hover, focus,
//! fill, …) delegates to this module so selector semantics are consistent:
//!
//! 1. **CSS selector** — default path, uses `DOM.querySelector`.
//! 2. **XPath** — prefix `//` or `/`, uses `Runtime.evaluate` with
//!    `document.evaluate()`.
//! 3. **Snapshot ref** — prefix `@e`, e.g. `@e5`. Not yet implemented;
//!    returns `UNSUPPORTED_OPERATION` until the snapshot annotation store
//!    is wired up.

use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::cdp_session::{CdpSession, cdp_error_to_result};

/// Resolve a `<selector>` string to a CDP `nodeId`.
///
/// Dispatches by selector form:
///   - `@eN`  → snapshot ref (placeholder)
///   - `//…`  → XPath
///   - `/…`   → XPath (absolute)
///   - else   → CSS selector
pub async fn resolve_node(
    cdp: &CdpSession,
    target_id: &str,
    selector: &str,
) -> Result<i64, ActionResult> {
    if selector.starts_with("@e") {
        resolve_ref(selector)
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
) -> Result<(f64, f64), ActionResult> {
    let node_id = resolve_node(cdp, target_id, selector).await?;
    get_element_center(cdp, target_id, node_id, selector).await
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

/// Snapshot ref (`@eN`) — placeholder until annotation store exists.
fn resolve_ref(selector: &str) -> Result<i64, ActionResult> {
    Err(ActionResult::fatal(
        "UNSUPPORTED_OPERATION",
        format!("snapshot refs are not yet supported: '{selector}'"),
    ))
}

// ── Error helper ───────────────────────────────────────────────────

fn element_not_found(selector: &str) -> ActionResult {
    ActionResult::Fatal {
        code: "ELEMENT_NOT_FOUND".to_string(),
        message: format!("element not found: {selector}"),
        hint: String::new(),
        details: Some(json!({ "selector": selector })),
    }
}

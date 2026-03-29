use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::action_result::ActionResult;
use crate::daemon::cdp_session::{CdpSession, cdp_error_to_result, get_cdp_and_target};
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

use super::snapshot_transform::RefCache;

/// Inspect the element at specified coordinates
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// Point to inspect as "x,y" (e.g. "100,200")
    #[arg(allow_hyphen_values = true)]
    pub coordinates: String,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
    /// Number of parent levels to trace upward
    #[arg(long)]
    pub parent_depth: Option<u32>,
}

pub const COMMAND_NAME: &str = "browser.inspect-point";

/// Parse coordinate string "x,y" into (f64, f64).
pub fn parse_coordinates(coords: &str) -> Result<(f64, f64), String> {
    let parts: Vec<&str> = coords.splitn(2, ',').collect();
    if parts.len() != 2 {
        return Err(format!(
            "invalid coordinates '{}': expected format 'x,y' (e.g. '100,200')",
            coords
        ));
    }
    let x = parts[0]
        .trim()
        .parse::<f64>()
        .map_err(|_| format!("invalid x coordinate '{}'", parts[0].trim()))?;
    let y = parts[1]
        .trim()
        .parse::<f64>()
        .map_err(|_| format!("invalid y coordinate '{}'", parts[1].trim()))?;
    Ok((x, y))
}

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    if let ActionResult::Fatal { code, .. } = result
        && code == "SESSION_NOT_FOUND"
    {
        return None;
    }
    let tab_id = if let ActionResult::Fatal { code, .. } = result
        && code == "TAB_NOT_FOUND"
    {
        None
    } else {
        Some(cmd.tab.clone())
    };
    let url = match result {
        ActionResult::Ok { data } => data
            .get("__ctx_url")
            .and_then(|v| v.as_str())
            .map(String::from),
        _ => None,
    };
    Some(ResponseContext {
        session_id: cmd.session.clone(),
        tab_id,
        window_id: None,
        url,
        title: None,
    })
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    // Validate coordinates early
    let (x, y) = match parse_coordinates(&cmd.coordinates) {
        Ok(v) => v,
        Err(e) => return ActionResult::fatal("INVALID_ARGUMENT", e),
    };

    let (cdp, target_id) = match get_cdp_and_target(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    let url = crate::browser::navigation::get_tab_url(&cdp, &target_id).await;

    // Get or create RefCache for this tab
    let mut ref_cache = {
        let mut reg = registry.lock().await;
        reg.take_ref_cache(&cmd.session, &cmd.tab)
    };

    let result = inspect_at_point(&cdp, &target_id, x, y, cmd.parent_depth, &mut ref_cache).await;

    // Store RefCache back
    {
        let mut reg = registry.lock().await;
        reg.put_ref_cache(&cmd.session, &cmd.tab, ref_cache);
    }

    match result {
        Ok((element, parents)) => ActionResult::ok(json!({
            "point": { "x": x, "y": y },
            "element": element,
            "parents": parents,
            "__ctx_url": url,
        })),
        Err(e) => e,
    }
}

/// Hit-test at (x, y) and return (element, parents).
///
/// Returns `Ok((null, []))` when no element is at the point.
async fn inspect_at_point(
    cdp: &CdpSession,
    target_id: &str,
    x: f64,
    y: f64,
    parent_depth: Option<u32>,
    ref_cache: &mut RefCache,
) -> Result<(Value, Value), ActionResult> {
    // Push the full DOM tree (depth -1 = all descendants) so that parentIds
    // are populated for DOM.describeNode calls in the parent-traversal path.
    let _ = cdp
        .execute_on_tab(target_id, "DOM.getDocument", json!({ "depth": -1 }))
        .await;

    // Use DOM.getNodeForLocation to find the element at (x, y).
    // Coordinates must be integers for CDP.
    let hit = cdp
        .execute_on_tab(
            target_id,
            "DOM.getNodeForLocation",
            json!({
                "x": x as i64,
                "y": y as i64,
                "includeUserAgentShadowDOM": false,
                "ignorePointerEventsNone": true,
            }),
        )
        .await;

    let (backend_node_id, hit_node_id) = match hit {
        Ok(ref v) => (
            v["result"]["backendNodeId"].as_i64(),
            v["result"]["nodeId"].as_i64().unwrap_or(0),
        ),
        Err(_) => (None, 0),
    };

    let Some(backend_node_id) = backend_node_id else {
        // No element at coordinates — return null element
        return Ok((Value::Null, json!([])));
    };

    // Get AX info for the element
    let element_info =
        get_ax_info_for_backend_node(cdp, target_id, backend_node_id, ref_cache).await?;

    // Collect parents if requested
    let parents = if let Some(depth) = parent_depth {
        if depth > 0 {
            collect_parents(cdp, target_id, hit_node_id, depth, ref_cache).await?
        } else {
            json!([])
        }
    } else {
        json!([])
    };

    Ok((element_info, parents))
}

/// Get AX role/name/selector for a backend node ID.
/// Returns a JSON object {role, name, selector}.
async fn get_ax_info_for_backend_node(
    cdp: &CdpSession,
    target_id: &str,
    backend_node_id: i64,
    ref_cache: &mut RefCache,
) -> Result<Value, ActionResult> {
    let ax_resp = cdp
        .execute_on_tab(
            target_id,
            "Accessibility.getPartialAXTree",
            json!({
                "backendNodeId": backend_node_id,
                "fetchRelatives": false,
            }),
        )
        .await
        .map_err(|e| cdp_error_to_result(e, "INTERNAL_ERROR"))?;

    let nodes = ax_resp["result"]["nodes"]
        .as_array()
        .and_then(|arr| arr.first());

    let (role, name) = if let Some(node) = nodes {
        let role = node["role"]["value"]
            .as_str()
            .unwrap_or("generic")
            .to_string();
        let name = node["name"]["value"].as_str().unwrap_or("").to_string();
        (role, name)
    } else {
        ("generic".to_string(), String::new())
    };

    // Assign stable ref from RefCache
    let selector = ref_cache.get_or_assign(backend_node_id);

    Ok(json!({
        "role": role,
        "name": name,
        "selector": selector,
    }))
}

/// Walk up the DOM parent chain, collecting up to `depth` ancestors.
/// Returns a JSON array of {role, name, selector} objects, nearest parent first.
///
/// Requires `DOM.getDocument` to have been called first so that nodeIds are
/// tracked. Uses `start_node_id` (from `DOM.getNodeForLocation` result.nodeId)
/// for reliable parentId traversal via iterative `DOM.describeNode` calls.
async fn collect_parents(
    cdp: &CdpSession,
    target_id: &str,
    start_node_id: i64,
    depth: u32,
    ref_cache: &mut RefCache,
) -> Result<Value, ActionResult> {
    if start_node_id == 0 {
        return Ok(json!([]));
    }

    let mut parents = Vec::new();
    let mut current_node_id = start_node_id;

    while parents.len() < depth as usize {
        // Describe current node to find its parentId
        let desc = cdp
            .execute_on_tab(
                target_id,
                "DOM.describeNode",
                json!({ "nodeId": current_node_id }),
            )
            .await;
        let desc = match desc {
            Ok(v) => v,
            Err(_) => break,
        };

        let parent_id = desc["result"]["node"]["parentId"].as_i64().unwrap_or(0);
        if parent_id == 0 {
            break;
        }

        // Describe the parent node
        let parent_desc = cdp
            .execute_on_tab(
                target_id,
                "DOM.describeNode",
                json!({ "nodeId": parent_id }),
            )
            .await;
        let parent_desc = match parent_desc {
            Ok(v) => v,
            Err(_) => break,
        };

        let node = &parent_desc["result"]["node"];
        let node_type = node["nodeType"].as_i64().unwrap_or(0);
        let parent_backend_id = node["backendNodeId"].as_i64().unwrap_or(0);

        // Stop at non-element nodes (document, doctype, etc.)
        if node_type != 1 || parent_backend_id == 0 {
            break;
        }

        let parent_info =
            get_ax_info_for_backend_node(cdp, target_id, parent_backend_id, ref_cache).await?;
        parents.push(parent_info);
        current_node_id = parent_id;
    }

    Ok(json!(parents))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_coordinates_valid() {
        assert_eq!(parse_coordinates("100,200"), Ok((100.0, 200.0)));
    }

    #[test]
    fn parse_coordinates_with_decimals() {
        assert_eq!(parse_coordinates("100.5,200.7"), Ok((100.5, 200.7)));
    }

    #[test]
    fn parse_coordinates_with_spaces() {
        assert_eq!(parse_coordinates(" 100 , 200 "), Ok((100.0, 200.0)));
    }

    #[test]
    fn parse_coordinates_negative() {
        assert_eq!(parse_coordinates("-10,20"), Ok((-10.0, 20.0)));
    }

    #[test]
    fn parse_coordinates_zero() {
        assert_eq!(parse_coordinates("0,0"), Ok((0.0, 0.0)));
    }

    #[test]
    fn parse_coordinates_missing_comma() {
        let err = parse_coordinates("100200").unwrap_err();
        assert!(err.contains("invalid coordinates"));
    }

    #[test]
    fn parse_coordinates_non_numeric_x() {
        let err = parse_coordinates("abc,200").unwrap_err();
        assert!(err.contains("invalid x coordinate"));
    }

    #[test]
    fn parse_coordinates_non_numeric_y() {
        let err = parse_coordinates("100,xyz").unwrap_err();
        assert!(err.contains("invalid y coordinate"));
    }

    #[test]
    fn parse_coordinates_empty() {
        let err = parse_coordinates("").unwrap_err();
        assert!(err.contains("invalid"));
    }

    #[test]
    fn parse_coordinates_extra_commas() {
        // splitn(2, ',') treats "1,2,3" as ["1", "2,3"] — "2,3" fails f64 parse
        let err = parse_coordinates("1,2,3").unwrap_err();
        assert!(err.contains("invalid y coordinate"));
    }
}

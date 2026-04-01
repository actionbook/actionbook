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
#[command(after_help = "\
Examples:
  actionbook browser inspect-point 420,310 --session s1 --tab t1
  actionbook browser inspect-point 420,310 --parent-depth 2 --session s1 --tab t1

Returns the element's role, name, and CSS selector at the given x,y point.
Use with screenshot to identify elements visually.")]
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

pub const COMMAND_NAME: &str = "browser inspect-point";

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

    let backend_node_id = match hit {
        Ok(ref v) => v["result"]["backendNodeId"].as_i64(),
        Err(_) => None,
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
            collect_parents(cdp, target_id, backend_node_id, depth, ref_cache).await?
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
    let selector = ref_cache.get_or_assign(backend_node_id, &role, &name, None);

    Ok(json!({
        "role": role,
        "name": name,
        "selector": selector,
    }))
}

/// Walk up the AX parent chain, collecting up to `depth` ancestors.
/// Returns a JSON array of {role, name, selector} objects, nearest parent first.
///
/// Uses `Accessibility.getPartialAXTree` with `fetchRelatives: true` to get
/// the element and all its AX ancestors in a single CDP call, then walks up
/// via `parentId` cross-references in the flat node list.
async fn collect_parents(
    cdp: &CdpSession,
    target_id: &str,
    backend_node_id: i64,
    depth: u32,
    ref_cache: &mut RefCache,
) -> Result<Value, ActionResult> {
    // Fetch the AX tree including ancestors.
    let ax_resp = cdp
        .execute_on_tab(
            target_id,
            "Accessibility.getPartialAXTree",
            json!({
                "backendNodeId": backend_node_id,
                "fetchRelatives": true,
            }),
        )
        .await;

    let nodes = match ax_resp {
        Ok(ref v) => v["result"]["nodes"].as_array().cloned().unwrap_or_default(),
        Err(_) => return Ok(json!([])),
    };

    if nodes.is_empty() {
        return Ok(json!([]));
    }

    // Build a map from AX nodeId → index in nodes array for O(1) lookups.
    let mut ax_id_to_idx: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for (i, node) in nodes.iter().enumerate() {
        if let Some(id) = node["nodeId"].as_str() {
            ax_id_to_idx.insert(id.to_string(), i);
        }
    }

    // The first node in the array is the requested element.
    // Walk up via parentId for up to `depth` steps.
    let mut parents = Vec::new();
    let mut current_ax_id = nodes[0]["nodeId"].as_str().map(String::from);

    while parents.len() < depth as usize {
        let current_id = match current_ax_id {
            Some(ref id) => id.clone(),
            None => break,
        };

        let current_idx = match ax_id_to_idx.get(&current_id) {
            Some(&idx) => idx,
            None => break,
        };

        let parent_ax_id = nodes[current_idx]["parentId"].as_str().map(String::from);
        let parent_ax_id = match parent_ax_id {
            Some(id) => id,
            None => break,
        };

        let parent_idx = match ax_id_to_idx.get(&parent_ax_id) {
            Some(&idx) => idx,
            None => break,
        };

        let parent_node = &nodes[parent_idx];

        // Skip AX nodes that represent the entire document/page root
        let role = parent_node["role"]["value"]
            .as_str()
            .unwrap_or("generic")
            .to_string();
        if role == "RootWebArea" || role == "WebArea" {
            break;
        }

        let name = parent_node["name"]["value"]
            .as_str()
            .unwrap_or("")
            .to_string();

        // Use backendDOMNodeId for stable ref assignment
        let backend_dom_id = parent_node["backendDOMNodeId"].as_i64().unwrap_or(0);
        let selector = if backend_dom_id != 0 {
            ref_cache.get_or_assign(backend_dom_id, &role, &name, None)
        } else {
            ref_cache.get_or_assign(parent_idx as i64, &role, &name, None)
        };

        parents.push(json!({
            "role": role,
            "name": name,
            "selector": selector,
        }));

        current_ax_id = Some(parent_ax_id);
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

use clap::Args;
use serde::{Deserialize, Serialize};

use crate::action_result::ActionResult;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Inspect the element at specified coordinates
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// Point to inspect as "x,y" (e.g. "100,200")
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
    if let Err(e) = parse_coordinates(&cmd.coordinates) {
        return ActionResult::fatal("INVALID_ARGUMENT", e);
    }

    // Resolve session/tab so error-path tests work against the stub
    let (_cdp, _target_id) = match crate::daemon::cdp_session::get_cdp_and_target(
        registry,
        &cmd.session,
        &cmd.tab,
    )
    .await
    {
        Ok(v) => v,
        Err(e) => return e,
    };

    ActionResult::fatal(
        "NOT_IMPLEMENTED",
        "browser.inspect-point not yet implemented",
    )
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

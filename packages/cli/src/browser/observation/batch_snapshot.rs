use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::action_result::ActionResult;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

use super::snapshot;

fn cursor_default() -> bool {
    true
}

/// Capture accessibility snapshots for multiple tabs in one request.
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser batch-snapshot --session s1 --tabs t1 t2 t3
  actionbook browser batch-snapshot --session s1 --tabs t1 t2 --interactive --compact")]
pub struct Cmd {
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab IDs
    #[arg(long, num_args = 1..)]
    #[serde(rename = "tab_ids")]
    pub tabs: Vec<String>,
    /// Include only interactive elements
    #[arg(long, short = 'i', default_value_t = false)]
    #[serde(default)]
    pub interactive: bool,
    /// Compact output, remove empty structural nodes
    #[arg(long, short = 'c', default_value_t = false)]
    #[serde(default)]
    pub compact: bool,
    /// Include cursor-interactive custom elements (cursor:pointer, onclick, tabindex) — enabled by default
    #[arg(long, default_value_t = true)]
    #[serde(default = "cursor_default")]
    pub cursor: bool,
    /// Limit maximum tree depth
    #[arg(long, short = 'd')]
    #[serde(default)]
    pub depth: Option<u32>,
    /// Limit to a specific subtree by CSS selector
    #[arg(long, short = 's')]
    #[serde(default)]
    pub selector: Option<String>,
}

pub const COMMAND_NAME: &str = "browser batch-snapshot";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    if let ActionResult::Fatal { code, .. } = result
        && code == "SESSION_NOT_FOUND"
    {
        return None;
    }

    Some(ResponseContext {
        session_id: cmd.session.clone(),
        tab_id: None,
        window_id: None,
        url: None,
        title: None,
    })
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    let mut results = Vec::new();

    for tab_id in &cmd.tabs {
        let tab_cmd = snapshot::Cmd {
            session: cmd.session.clone(),
            tab: tab_id.clone(),
            interactive: cmd.interactive,
            compact: cmd.compact,
            cursor: cmd.cursor,
            depth: cmd.depth,
            selector: cmd.selector.clone(),
        };
        match snapshot::execute(&tab_cmd, registry).await {
            ActionResult::Ok { data } => {
                let nodes = data["nodes"].as_array().cloned().unwrap_or_default();
                let node_count = data["stats"]["node_count"].as_u64().unwrap_or(0);
                let interactive_count = data["stats"]["interactive_count"].as_u64().unwrap_or(0);
                let url = data["__ctx_url"].as_str().unwrap_or("");
                let title = data["__ctx_title"].as_str().unwrap_or("");
                let path = data["path"].as_str().unwrap_or("");
                results.push(build_tab_ok(
                    tab_id,
                    path,
                    nodes,
                    node_count,
                    interactive_count,
                    url,
                    title,
                ));
            }
            ActionResult::Fatal { ref code, .. } if code == "SESSION_NOT_FOUND" => {
                return session_not_found_result(&cmd.session);
            }
            ActionResult::Fatal { code, message, .. } => {
                results.push(build_tab_error(tab_id, &code, &message));
            }
            ActionResult::Retryable { reason, .. } => {
                results.push(build_tab_error(tab_id, "RETRYABLE", &reason));
            }
            ActionResult::UserAction { action, hint } => {
                results.push(build_tab_error(
                    tab_id,
                    "USER_ACTION_REQUIRED",
                    &format!("{action}: {hint}"),
                ));
            }
        }
    }

    build_batch_snapshot_ok(results)
}

fn build_batch_snapshot_ok(results: Vec<Value>) -> ActionResult {
    ActionResult::ok(json!({
        "format": "batch-snapshot",
        "results": results,
    }))
}

fn build_tab_ok(
    tab_id: &str,
    path: &str,
    nodes: Vec<Value>,
    node_count: u64,
    interactive_count: u64,
    url: &str,
    title: &str,
) -> Value {
    json!({
        "tab_id": tab_id,
        "status": "ok",
        "path": path,
        "nodes": nodes,
        "stats": {
            "node_count": node_count,
            "interactive_count": interactive_count,
        },
        "__ctx_url": url,
        "__ctx_title": title,
    })
}

fn build_tab_error(tab_id: &str, code: &str, message: &str) -> Value {
    json!({
        "tab_id": tab_id,
        "status": "error",
        "code": code,
        "message": message,
    })
}

fn session_not_found_result(session: &str) -> ActionResult {
    ActionResult::fatal(
        "SESSION_NOT_FOUND",
        format!("session '{session}' not found or not active"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_nodes() -> Vec<Value> {
        vec![
            json!({
                "ref": "e1",
                "role": "link",
                "name": "Docs",
                "value": "",
            }),
            json!({
                "ref": "e2",
                "role": "button",
                "name": "Go",
                "value": "",
            }),
        ]
    }

    #[test]
    fn test_batch_snapshot_partial_success_returns_ok_with_mixed_results() {
        let results = vec![
            build_tab_ok(
                "t1",
                "/tmp/snapshot_t1_123.yaml",
                sample_nodes(),
                2,
                1,
                "https://example.com/a",
                "Page A",
            ),
            build_tab_error("t2", "TAB_NOT_FOUND", "tab t2 not found"),
        ];

        let result = build_batch_snapshot_ok(results);
        let data = match result {
            ActionResult::Ok { data } => data,
            other => panic!("expected Ok, got {other:?}"),
        };

        assert_eq!(data["format"], "batch-snapshot");
        let results = data["results"]
            .as_array()
            .expect("results must be an array");
        assert_eq!(results.len(), 2);

        assert_eq!(results[0]["tab_id"], "t1");
        assert_eq!(results[0]["status"], "ok");
        assert_eq!(results[0]["path"], "/tmp/snapshot_t1_123.yaml");
        assert_eq!(results[0]["stats"]["node_count"], 2);
        assert_eq!(results[0]["stats"]["interactive_count"], 1);
        assert_eq!(results[0]["__ctx_url"], "https://example.com/a");
        assert_eq!(results[0]["__ctx_title"], "Page A");
        assert!(results[0]["nodes"].is_array());

        assert_eq!(results[1]["tab_id"], "t2");
        assert_eq!(results[1]["status"], "error");
        assert_eq!(results[1]["code"], "TAB_NOT_FOUND");
        assert_eq!(results[1]["message"], "tab t2 not found");
    }

    #[test]
    fn test_batch_snapshot_all_success_keeps_every_result_ok() {
        let result = build_batch_snapshot_ok(vec![
            build_tab_ok(
                "t1",
                "/tmp/snapshot_t1.yaml",
                sample_nodes(),
                2,
                1,
                "https://example.com/1",
                "One",
            ),
            build_tab_ok(
                "t2",
                "/tmp/snapshot_t2.yaml",
                sample_nodes(),
                2,
                1,
                "https://example.com/2",
                "Two",
            ),
        ]);

        let data = match result {
            ActionResult::Ok { data } => data,
            other => panic!("expected Ok, got {other:?}"),
        };
        let results = data["results"]
            .as_array()
            .expect("results must be an array");

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|entry| entry["status"] == "ok"));
    }

    #[test]
    fn test_batch_snapshot_all_fail_still_returns_ok_envelope() {
        let result = build_batch_snapshot_ok(vec![
            build_tab_error("t1", "TAB_NOT_FOUND", "tab t1 not found"),
            build_tab_error("t2", "TAB_NOT_FOUND", "tab t2 not found"),
        ]);

        let data = match result {
            ActionResult::Ok { data } => data,
            other => panic!("expected Ok, got {other:?}"),
        };
        let results = data["results"]
            .as_array()
            .expect("results must be an array");

        assert_eq!(data["format"], "batch-snapshot");
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|entry| entry["status"] == "error"));
        assert!(results.iter().all(|entry| entry["code"] == "TAB_NOT_FOUND"));
    }

    #[test]
    fn test_batch_snapshot_session_not_found_returns_fatal() {
        let result = session_not_found_result("missing-session");

        match result {
            ActionResult::Fatal { code, message, .. } => {
                assert_eq!(code, "SESSION_NOT_FOUND");
                assert!(message.contains("missing-session"));
            }
            other => panic!("expected Fatal, got {other:?}"),
        }
    }

    #[test]
    fn test_batch_snapshot_ok_results_match_contract_shape() {
        let result = build_batch_snapshot_ok(vec![build_tab_ok(
            "t1",
            "/tmp/snapshot_t1.yaml",
            sample_nodes(),
            2,
            1,
            "https://example.com/1",
            "One",
        )]);

        let data = match result {
            ActionResult::Ok { data } => data,
            other => panic!("expected Ok, got {other:?}"),
        };
        let entry = &data["results"][0];

        assert_eq!(data["format"], "batch-snapshot");
        assert_eq!(entry["tab_id"], "t1");
        assert_eq!(entry["status"], "ok");
        assert!(entry["path"].is_string());
        assert!(entry["nodes"].is_array());
        assert!(entry["stats"].is_object());
        assert!(entry["stats"]["node_count"].is_number());
        assert!(entry["stats"]["interactive_count"].is_number());
    }
}

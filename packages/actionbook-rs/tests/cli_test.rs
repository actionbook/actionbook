//! CLI argument parsing tests
//!
//! These tests verify that CLI arguments are parsed correctly,
//! matching the behavior of the original TypeScript CLI.

#![allow(deprecated)]

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

/// Get the actionbook binary command
fn actionbook() -> Command {
    Command::cargo_bin("actionbook").unwrap()
}

/// Create an isolated environment for tests that touch the filesystem.
fn create_isolated_env() -> (tempfile::TempDir, String, String, String) {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let config_home = tmp.path().join("config");
    let data_home = tmp.path().join("data");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&config_home).unwrap();
    fs::create_dir_all(&data_home).unwrap();
    (
        tmp,
        home.to_string_lossy().to_string(),
        config_home.to_string_lossy().to_string(),
        data_home.to_string_lossy().to_string(),
    )
}

mod help {
    use super::*;

    #[test]
    fn shows_help() {
        actionbook()
            .arg("--help")
            .assert()
            .success()
            .stdout(predicate::str::contains("actionbook"))
            .stdout(predicate::str::contains("Browser automation"));
    }

    #[test]
    fn shows_version() {
        actionbook()
            .arg("--version")
            .assert()
            .success()
            .stdout(predicate::str::contains("actionbook"));
    }

    #[test]
    fn help_lists_all_commands() {
        actionbook()
            .arg("--help")
            .assert()
            .success()
            .stdout(predicate::str::contains("browser"))
            .stdout(predicate::str::contains("search"))
            .stdout(predicate::str::contains("get"))
            .stdout(predicate::str::contains("sources"))
            .stdout(predicate::str::contains("config"))
            .stdout(predicate::str::contains("profile"))
            .stdout(predicate::str::contains("extension"))
            .stdout(predicate::str::contains("setup"));
    }

    #[test]
    fn help_lists_global_options() {
        actionbook()
            .arg("--help")
            .assert()
            .success()
            .stdout(predicate::str::contains("--browser-path"))
            .stdout(predicate::str::contains("--cdp"))
            .stdout(predicate::str::contains("--profile"))
            .stdout(predicate::str::contains("--headless"))
            .stdout(predicate::str::contains("--api-key"))
            .stdout(predicate::str::contains("--json"))
            .stdout(predicate::str::contains("--verbose"));
    }
}

mod search_command {
    use super::*;

    #[test]
    fn search_requires_query() {
        actionbook()
            .arg("search")
            .assert()
            .failure()
            .stderr(predicate::str::contains("QUERY"));
    }

    #[test]
    fn search_help_shows_options() {
        actionbook()
            .args(["search", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--domain"))
            .stdout(predicate::str::contains("--url"))
            .stdout(predicate::str::contains("--page"))
            .stdout(predicate::str::contains("--page-size"));
    }

    #[test]
    fn search_accepts_domain_flag() {
        // Just check that the flag is accepted (API call may fail)
        actionbook()
            .args(["search", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("airbnb.com"));
    }

    #[test]
    fn search_page_size_has_default() {
        actionbook()
            .args(["search", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("[default: 10]"));
    }
}

mod get_command {
    use super::*;

    #[test]
    fn get_requires_area_id() {
        actionbook()
            .arg("get")
            .assert()
            .failure()
            .stderr(predicate::str::contains("AREA_ID"));
    }

    #[test]
    fn get_help_shows_usage() {
        actionbook()
            .args(["get", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Area ID"));
    }
}

mod sources_command {
    use super::*;

    #[test]
    fn sources_requires_subcommand() {
        actionbook()
            .arg("sources")
            .assert()
            .failure()
            .stderr(predicate::str::contains("subcommand"));
    }

    #[test]
    fn sources_list_help() {
        actionbook()
            .args(["sources", "list", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("List all sources"));
    }

    #[test]
    fn sources_search_requires_query() {
        actionbook()
            .args(["sources", "search"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("QUERY"));
    }
}

mod browser_command {
    use super::*;
    use futures::{SinkExt, StreamExt};
    use serial_test::serial;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    fn setup_config(default_profile: &str) -> (tempfile::TempDir, String, String, String) {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let config_home = tmp.path().join("config");
        let data_home = tmp.path().join("data");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&config_home).unwrap();
        fs::create_dir_all(&data_home).unwrap();

        let config_path_output = actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["config", "path"])
            .output()
            .unwrap();
        assert!(
            config_path_output.status.success(),
            "failed to resolve config path: {}",
            String::from_utf8_lossy(&config_path_output.stderr)
        );
        let config_path = String::from_utf8_lossy(&config_path_output.stdout)
            .trim()
            .to_string();
        let config_file = std::path::PathBuf::from(config_path);
        fs::create_dir_all(config_file.parent().unwrap()).unwrap();

        let config = format!(
            r#"[api]
base_url = "https://api.actionbook.dev"

[browser]
headless = false
default_profile = "{}"
"#,
            default_profile
        );
        fs::write(config_file, config).unwrap();

        (
            tmp,
            home.to_string_lossy().to_string(),
            config_home.to_string_lossy().to_string(),
            data_home.to_string_lossy().to_string(),
        )
    }

    fn write_session_state(
        home: &str,
        profile: &str,
        cdp_url: &str,
        ws_headers: Option<serde_json::Value>,
    ) {
        let sessions_dir = std::path::Path::new(home)
            .join(".actionbook")
            .join("sessions");
        fs::create_dir_all(&sessions_dir).unwrap();
        let mut state = serde_json::json!({
            "profile_name": profile,
            "cdp_port": 9222,
            "pid": serde_json::Value::Null,
            "cdp_url": cdp_url,
            "active_page_id": "page-1",
        });
        if let Some(ws_headers) = ws_headers {
            state["ws_headers"] = ws_headers;
        }
        fs::write(
            sessions_dir.join(format!("{profile}.json")),
            serde_json::to_vec_pretty(&state).unwrap(),
        )
        .unwrap();
    }

    fn spawn_remote_cdp_server() -> (
        u16,
        Arc<Mutex<Vec<serde_json::Value>>>,
        std::thread::JoinHandle<()>,
    ) {
        spawn_remote_cdp_server_with_initial_url("https://existing.example")
    }

    fn spawn_remote_cdp_server_with_initial_url(
        initial_url: &str,
    ) -> (
        u16,
        Arc<Mutex<Vec<serde_json::Value>>>,
        std::thread::JoinHandle<()>,
    ) {
        let (tx, rx) = std::sync::mpsc::channel();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let server_requests = Arc::clone(&requests);
        let initial_url = initial_url.to_string();

        let handle = std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                tx.send(listener.local_addr().unwrap().port()).unwrap();

                let mut pages = vec![serde_json::json!({
                    "targetId": "page-1",
                    "type": "page",
                    "title": if initial_url == "about:blank" { "Blank Page" } else { "Existing Page" },
                    "url": initial_url,
                })];
                let mut next_page_id = 2usize;

                loop {
                    let accepted =
                        tokio::time::timeout(Duration::from_secs(2), listener.accept()).await;
                    let Ok(Ok((stream, _))) = accepted else {
                        break;
                    };

                    let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();

                    while let Some(message) = ws.next().await {
                        let message = match message {
                            Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => text,
                            Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => break,
                            Ok(_) => continue,
                            Err(_) => break,
                        };

                        let request: serde_json::Value =
                            serde_json::from_str(message.as_str()).unwrap();
                        server_requests.lock().unwrap().push(request.clone());
                        let method = request
                            .get("method")
                            .and_then(|value| value.as_str())
                            .unwrap_or("");

                        let response = match method {
                            "Target.getTargets" => serde_json::json!({
                                "id": request.get("id").cloned().unwrap_or_else(|| serde_json::json!(1)),
                                "result": {
                                    "targetInfos": pages.clone()
                                }
                            }),
                            "Target.attachToTarget" => serde_json::json!({
                                "id": request.get("id").cloned().unwrap_or_else(|| serde_json::json!(1)),
                                "result": {
                                    "sessionId": "session-1"
                                }
                            }),
                            "Target.createTarget" => {
                                if request.get("sessionId").is_some() {
                                    serde_json::json!({
                                        "id": request.get("id").cloned().unwrap_or_else(|| serde_json::json!(2)),
                                        "error": {
                                            "message": "Target.createTarget must be sent without sessionId"
                                        }
                                    })
                                } else {
                                    let new_page_id = format!("page-{}", next_page_id);
                                    next_page_id += 1;
                                    let url = request
                                        .get("params")
                                        .and_then(|params| params.get("url"))
                                        .and_then(|value| value.as_str())
                                        .unwrap_or("about:blank")
                                        .to_string();
                                    pages.push(serde_json::json!({
                                        "targetId": new_page_id,
                                        "type": "page",
                                        "title": "Opened Page",
                                        "url": url,
                                    }));
                                    serde_json::json!({
                                        "id": request.get("id").cloned().unwrap_or_else(|| serde_json::json!(2)),
                                        "result": {
                                            "targetId": format!("page-{}", next_page_id - 1)
                                        }
                                    })
                                }
                            }
                            "Page.navigate" => {
                                let url = request
                                    .get("params")
                                    .and_then(|p| p.get("url"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("about:blank")
                                    .to_string();
                                // Update the active page URL in the pages list
                                if let Some(page) = pages.first_mut() {
                                    page["url"] = serde_json::json!(url);
                                    page["title"] = serde_json::json!("Navigated Page");
                                }
                                serde_json::json!({
                                    "id": request.get("id").cloned().unwrap_or_else(|| serde_json::json!(1)),
                                    "result": {
                                        "frameId": "frame-1",
                                        "loaderId": "loader-1"
                                    }
                                })
                            }
                            "Accessibility.getFullAXTree" => serde_json::json!({
                                "id": request.get("id").cloned().unwrap_or_else(|| serde_json::json!(1)),
                                "result": {
                                    "nodes": [
                                        {
                                            "nodeId": "1",
                                            "ignored": false,
                                            "role": { "type": "role", "value": "RootWebArea" },
                                            "name": { "type": "computedString", "value": "Test Page" },
                                            "childIds": ["2"],
                                            "properties": []
                                        },
                                        {
                                            "nodeId": "2",
                                            "ignored": false,
                                            "role": { "type": "role", "value": "heading" },
                                            "name": { "type": "computedString", "value": "Hello World" },
                                            "childIds": [],
                                            "properties": [
                                                { "name": "level", "value": { "type": "integer", "value": 1 } }
                                            ]
                                        }
                                    ]
                                }
                            }),
                            "Page.getNavigationHistory" => serde_json::json!({
                                "id": request.get("id").cloned().unwrap_or_else(|| serde_json::json!(1)),
                                "result": {
                                    "currentIndex": 0,
                                    "entries": [{
                                        "id": 0,
                                        "url": pages.first().map(|p| p["url"].as_str().unwrap_or("about:blank")).unwrap_or("about:blank"),
                                        "title": "Test Page"
                                    }]
                                }
                            }),
                            "Page.addScriptToEvaluateOnNewDocument" => serde_json::json!({
                                "id": request.get("id").cloned().unwrap_or_else(|| serde_json::json!(2)),
                                "result": {
                                    "identifier": "stealth-script"
                                }
                            }),
                            "Runtime.evaluate" => {
                                let expression = request
                                    .get("params")
                                    .and_then(|params| params.get("expression"))
                                    .and_then(|value| value.as_str())
                                    .unwrap_or("");
                                let value = if expression.contains("document.readyState") {
                                    serde_json::json!("complete")
                                } else if expression.contains("document.title") {
                                    serde_json::json!("Opened Page")
                                } else {
                                    serde_json::Value::Null
                                };
                                serde_json::json!({
                                    "id": request.get("id").cloned().unwrap_or_else(|| serde_json::json!(2)),
                                    "result": {
                                        "result": {
                                            "value": value
                                        }
                                    }
                                })
                            }
                            // Return empty success for unhandled methods (e.g. Target.setAutoAttach,
                            // Target.setDiscoverTargets) that the CLI sends but tests don't need
                            // to assert on.
                            _ => serde_json::json!({
                                "id": request.get("id").cloned().unwrap_or_else(|| serde_json::json!(1)),
                                "result": {}
                            }),
                        };

                        ws.send(tokio_tungstenite::tungstenite::Message::Text(
                            response.to_string().into(),
                        ))
                        .await
                        .unwrap();
                    }
                }
            });
        });

        (rx.recv().unwrap(), requests, handle)
    }

    // ── Subcommand requirement ──────────────────────────────────────

    #[test]
    fn browser_requires_subcommand() {
        actionbook()
            .arg("browser")
            .assert()
            .failure()
            .stderr(predicate::str::contains("subcommand"));
    }

    // ── Argument validation ─────────────────────────────────────────

    #[test]
    fn browser_open_requires_url() {
        actionbook()
            .args(["browser", "open"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("URL"));
    }

    #[test]
    fn browser_goto_requires_url() {
        actionbook()
            .args(["browser", "goto"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("URL"));
    }

    #[test]
    fn browser_click_requires_selector() {
        actionbook()
            .args(["browser", "click"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("SELECTOR"));
    }

    #[test]
    fn browser_type_requires_selector() {
        actionbook()
            .args(["browser", "type"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("SELECTOR"));
    }

    #[test]
    fn browser_type_requires_text() {
        actionbook()
            .args(["browser", "type", "#input"])
            .assert()
            .failure()
            .stderr(predicate::str::is_match("(?i)TEXT|required").unwrap());
    }

    #[test]
    fn browser_fill_requires_selector() {
        actionbook()
            .args(["browser", "fill"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("SELECTOR"));
    }

    #[test]
    fn browser_fill_requires_text() {
        actionbook()
            .args(["browser", "fill", "#input"])
            .assert()
            .failure()
            .stderr(predicate::str::is_match("(?i)TEXT|required").unwrap());
    }

    #[test]
    fn browser_select_requires_selector() {
        actionbook()
            .args(["browser", "select"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("SELECTOR"));
    }

    #[test]
    fn browser_select_requires_value() {
        actionbook()
            .args(["browser", "select", "#foo"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("VALUE"));
    }

    #[test]
    fn browser_hover_requires_selector() {
        actionbook()
            .args(["browser", "hover"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("SELECTOR"));
    }

    #[test]
    fn browser_focus_requires_selector() {
        actionbook()
            .args(["browser", "focus"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("SELECTOR"));
    }

    #[test]
    fn browser_press_requires_key() {
        actionbook()
            .args(["browser", "press"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("KEY"));
    }

    #[test]
    fn browser_switch_requires_page_id() {
        actionbook()
            .args(["browser", "switch"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("PAGE_ID"));
    }

    #[test]
    fn browser_wait_requires_selector() {
        actionbook()
            .args(["browser", "wait"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("SELECTOR"));
    }

    #[test]
    fn browser_eval_requires_code() {
        actionbook()
            .args(["browser", "eval"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("CODE"));
    }

    #[test]
    fn browser_pdf_requires_path() {
        actionbook()
            .args(["browser", "pdf"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("PATH"));
    }

    #[test]
    fn browser_inspect_requires_coordinates() {
        actionbook()
            .args(["browser", "inspect"])
            .assert()
            .failure()
            .stderr(predicate::str::is_match("[XY]").unwrap());
    }

    #[test]
    fn browser_connect_requires_endpoint() {
        actionbook()
            .args(["browser", "connect"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("ENDPOINT"));
    }

    // ── Scroll argument validation ──────────────────────────────────

    #[test]
    fn browser_scroll_requires_subcommand() {
        actionbook()
            .args(["browser", "scroll"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("subcommand"));
    }

    #[test]
    fn browser_scroll_to_requires_selector() {
        actionbook()
            .args(["browser", "scroll", "to"])
            .assert()
            .failure()
            .stderr(predicate::str::is_match("(?i)SELECTOR|required").unwrap());
    }

    // ── Cookies argument validation ─────────────────────────────────

    #[test]
    fn browser_cookies_get_requires_name() {
        actionbook()
            .args(["browser", "cookies", "get"])
            .assert()
            .failure()
            .stderr(predicate::str::is_match("(?i)NAME|required").unwrap());
    }

    #[test]
    fn browser_cookies_set_requires_name() {
        actionbook()
            .args(["browser", "cookies", "set"])
            .assert()
            .failure()
            .stderr(predicate::str::is_match("(?i)NAME|required").unwrap());
    }

    #[test]
    fn browser_cookies_delete_requires_name() {
        actionbook()
            .args(["browser", "cookies", "delete"])
            .assert()
            .failure()
            .stderr(predicate::str::is_match("(?i)NAME|required").unwrap());
    }

    // ── Help output ─────────────────────────────────────────────────

    #[test]
    fn browser_status_help() {
        actionbook()
            .args(["browser", "status", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("browser status"));
    }

    #[test]
    fn browser_back_help() {
        actionbook()
            .args(["browser", "back", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());
    }

    #[test]
    fn browser_forward_help() {
        actionbook()
            .args(["browser", "forward", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());
    }

    #[test]
    fn browser_reload_help() {
        actionbook()
            .args(["browser", "reload", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());
    }

    #[test]
    fn browser_pages_help() {
        actionbook()
            .args(["browser", "pages", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());
    }

    #[test]
    fn browser_wait_nav_help_shows_timeout() {
        actionbook()
            .args(["browser", "wait-nav", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("timeout"));
    }

    #[test]
    fn browser_html_help_shows_selector() {
        actionbook()
            .args(["browser", "html", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::is_match("(?i)selector").unwrap());
    }

    #[test]
    fn browser_text_help_shows_selector() {
        actionbook()
            .args(["browser", "text", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::is_match("(?i)selector").unwrap());
    }

    #[test]
    fn browser_viewport_help() {
        actionbook()
            .args(["browser", "viewport", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());
    }

    #[test]
    fn browser_close_help() {
        actionbook()
            .args(["browser", "close", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());
    }

    #[test]
    fn browser_restart_help() {
        actionbook()
            .args(["browser", "restart", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());
    }

    #[test]
    fn browser_snapshot_help() {
        actionbook()
            .args(["browser", "snapshot", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("snapshot"));
    }

    #[test]
    fn browser_inspect_help_shows_x_y_and_desc() {
        actionbook()
            .args(["browser", "inspect", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::is_match("[Xx]").unwrap())
            .stdout(predicate::str::is_match("[Yy]").unwrap())
            .stdout(predicate::str::contains("--desc"));
    }

    #[test]
    fn browser_connect_help() {
        actionbook()
            .args(["browser", "connect", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Connect"));
    }

    #[test]
    fn browser_cookies_subcommands() {
        actionbook()
            .args(["browser", "cookies", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("list"))
            .stdout(predicate::str::contains("get"))
            .stdout(predicate::str::contains("set"))
            .stdout(predicate::str::contains("delete"))
            .stdout(predicate::str::contains("clear"));
    }

    #[test]
    fn browser_cookies_clear_help() {
        actionbook()
            .args(["browser", "cookies", "clear", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::is_match("(?i)clear|cookies").unwrap());
    }

    // ── Scroll help output ──────────────────────────────────────────

    #[test]
    fn browser_scroll_help_shows_smooth() {
        actionbook()
            .args(["browser", "scroll", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--smooth"));
    }

    #[test]
    fn browser_scroll_down_help() {
        actionbook()
            .args(["browser", "scroll", "down", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::is_match("(?i)scroll.*down|pixels").unwrap());
    }

    #[test]
    fn browser_scroll_up_help() {
        actionbook()
            .args(["browser", "scroll", "up", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::is_match("(?i)scroll.*up|pixels").unwrap());
    }

    #[test]
    fn browser_scroll_bottom_help() {
        actionbook()
            .args(["browser", "scroll", "bottom", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::is_match("(?i)scroll.*bottom|page").unwrap());
    }

    #[test]
    fn browser_scroll_top_help() {
        actionbook()
            .args(["browser", "scroll", "top", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::is_match("(?i)scroll.*top|page").unwrap());
    }

    #[test]
    fn browser_scroll_to_help_shows_align() {
        actionbook()
            .args(["browser", "scroll", "to", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--align"));
    }

    // ── Default values and flags ────────────────────────────────────

    #[test]
    fn browser_goto_help_shows_default_timeout() {
        actionbook()
            .args(["browser", "goto", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("[default: 30000]"));
    }

    #[test]
    fn browser_click_help_shows_wait_option() {
        actionbook()
            .args(["browser", "click", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--wait"));
    }

    #[test]
    fn browser_type_help_shows_wait_option() {
        actionbook()
            .args(["browser", "type", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--wait"));
    }

    #[test]
    fn browser_screenshot_help_shows_full_page_and_default() {
        actionbook()
            .args(["browser", "screenshot", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--full-page"))
            .stdout(predicate::str::contains("screenshot.png"));
    }

    // ── Browser status output ───────────────────────────────────────

    #[test]
    fn browser_status_shows_detected_browsers() {
        actionbook()
            .args(["browser", "status"])
            .timeout(std::time::Duration::from_secs(10))
            .assert()
            .success()
            .stdout(predicate::str::contains("Detected Browsers"));
    }

    #[test]
    fn browser_status_shows_session_status() {
        actionbook()
            .args(["browser", "status"])
            .timeout(std::time::Duration::from_secs(10))
            .assert()
            .success()
            .stdout(predicate::str::contains("Session Status"));
    }

    #[test]
    fn browser_verbose_status_runs() {
        actionbook()
            .args(["--verbose", "browser", "status"])
            .timeout(std::time::Duration::from_secs(10))
            .assert()
            .success();
    }

    // ── Connect validation ──────────────────────────────────────────

    #[test]
    fn browser_connect_invalid_endpoint_fails() {
        actionbook()
            .args(["browser", "connect", "not-a-port"])
            .timeout(std::time::Duration::from_secs(5))
            .assert()
            .failure()
            .stderr(predicate::str::contains("Invalid endpoint"));
    }

    #[test]
    fn browser_connect_unreachable_port_fails() {
        actionbook()
            .args(["browser", "connect", "19999"])
            .timeout(std::time::Duration::from_secs(10))
            .assert()
            .failure();
    }

    // ── Connect profile precedence ──────────────────────────────────

    #[test]
    fn browser_connect_uses_config_default_profile_when_not_specified() {
        // connect to unreachable endpoint fails with exit code 1 but still
        // emits JSON with the resolved profile name before exiting.
        let (_tmp, home, config_home, data_home) = setup_config("team");
        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args([
                "--json",
                "browser",
                "connect",
                "ws://127.0.0.1:9222/devtools/browser/test",
            ])
            .assert()
            .failure()
            .stdout(predicate::str::contains("\"profile\":\"team\""));
    }

    #[test]
    fn browser_connect_uses_env_profile_over_config_default() {
        let (_tmp, home, config_home, data_home) = setup_config("team");
        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .env("ACTIONBOOK_PROFILE", "env-profile")
            .args([
                "--json",
                "browser",
                "connect",
                "ws://127.0.0.1:9222/devtools/browser/test",
            ])
            .assert()
            .failure()
            .stdout(predicate::str::contains("\"profile\":\"env-profile\""));
    }

    #[test]
    fn browser_connect_cli_profile_overrides_env_and_config() {
        let (_tmp, home, config_home, data_home) = setup_config("team");
        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .env("ACTIONBOOK_PROFILE", "env-profile")
            .args([
                "--json",
                "--profile",
                "cli-profile",
                "browser",
                "connect",
                "ws://127.0.0.1:9222/devtools/browser/test",
            ])
            .assert()
            .failure()
            .stdout(predicate::str::contains("\"profile\":\"cli-profile\""));
    }

    #[test]
    fn empty_actionbook_session_env_is_treated_as_unset() {
        let (_tmp, home, config_home, data_home) = setup_config("team");
        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .env("ACTIONBOOK_SESSION", "")
            .args(["--json", "config", "path"])
            .assert()
            .success()
            .stdout(predicate::str::contains("\"path\""));
    }

    #[test]
    #[serial]
    fn browser_open_reuses_connected_remote_session_with_headers() {
        let (_tmp, home, config_home, data_home) = setup_config("team");
        let (port, _requests, server) = spawn_remote_cdp_server();
        let ws_url = format!("ws://127.0.0.1:{port}/automation");
        write_session_state(
            &home,
            "team",
            &ws_url,
            Some(serde_json::json!({ "x-test-auth": "secret" })),
        );

        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["--no-daemon", "browser", "open", "https://example.com"])
            .timeout(Duration::from_secs(10))
            .assert()
            .success()
            .stdout(predicate::str::contains("https://example.com"));

        server.join().unwrap();
    }

    #[test]
    #[serial]
    fn browser_open_reuses_connected_remote_session_without_headers() {
        let (_tmp, home, config_home, data_home) = setup_config("team");
        let (port, _requests, server) = spawn_remote_cdp_server();
        let ws_url = format!("ws://127.0.0.1:{port}/automation");
        write_session_state(&home, "team", &ws_url, None);

        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["--no-daemon", "browser", "open", "https://example.com"])
            .timeout(Duration::from_secs(10))
            .assert()
            .success()
            .stdout(predicate::str::contains("https://example.com"));

        server.join().unwrap();
    }

    #[cfg(feature = "stealth")]
    #[test]
    #[serial]
    fn browser_open_remote_session_with_headers_preserves_stealth() {
        let (_tmp, home, config_home, data_home) = setup_config("team");
        let (port, requests, server) = spawn_remote_cdp_server();
        let ws_url = format!("ws://127.0.0.1:{port}/automation");
        write_session_state(
            &home,
            "team",
            &ws_url,
            Some(serde_json::json!({ "x-test-auth": "secret" })),
        );

        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args([
                "--no-daemon",
                "--stealth",
                "browser",
                "open",
                "https://example.com",
            ])
            .timeout(Duration::from_secs(10))
            .assert()
            .success()
            .stdout(predicate::str::contains("https://example.com"));

        server.join().unwrap();

        let requests = requests.lock().unwrap();
        assert!(requests.iter().any(|request| {
            request.get("method").and_then(|value| value.as_str())
                == Some("Page.addScriptToEvaluateOnNewDocument")
        }));
        assert!(requests.iter().any(|request| {
            request.get("method").and_then(|value| value.as_str()) == Some("Runtime.evaluate")
                && request
                    .get("params")
                    .and_then(|params| params.get("expression"))
                    .and_then(|value| value.as_str())
                    .is_some_and(|expression| expression.contains("navigator, 'webdriver'"))
        }));
    }

    #[cfg(feature = "stealth")]
    #[test]
    #[serial]
    fn browser_open_reused_initial_blank_remote_tab_preserves_stealth() {
        let (_tmp, home, config_home, data_home) = setup_config("team");
        let (port, requests, server) = spawn_remote_cdp_server_with_initial_url("about:blank");
        let ws_url = format!("ws://127.0.0.1:{port}/automation");
        write_session_state(
            &home,
            "team",
            &ws_url,
            Some(serde_json::json!({ "x-test-auth": "secret" })),
        );

        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args([
                "--no-daemon",
                "--stealth",
                "browser",
                "open",
                "https://example.com",
            ])
            .timeout(Duration::from_secs(10))
            .assert()
            .success()
            .stdout(predicate::str::contains("https://example.com"));

        server.join().unwrap();

        let requests = requests.lock().unwrap();
        assert!(requests.iter().any(|request| {
            request.get("method").and_then(|value| value.as_str())
                == Some("Page.addScriptToEvaluateOnNewDocument")
        }));
        assert!(requests.iter().any(|request| {
            request.get("method").and_then(|value| value.as_str()) == Some("Runtime.evaluate")
                && request
                    .get("params")
                    .and_then(|params| params.get("expression"))
                    .and_then(|value| value.as_str())
                    .is_some_and(|expression| expression.contains("navigator, 'webdriver'"))
        }));
    }

    // ── B. Remote WSS: connect → goto → snapshot / connect → open → snapshot ──

    /// Helper: run connect → goto → snapshot against a mock CDP server.
    /// Returns (connect_ok, goto_ok, snapshot_ok, snapshot_stdout).
    fn run_connect_goto_snapshot(
        home: &str,
        config_home: &str,
        data_home: &str,
        ws_url: &str,
        profile: Option<&str>,
    ) -> (bool, bool, bool, String) {
        let mut connect_args: Vec<&str> =
            vec!["--no-daemon", "--json", "--browser-mode", "isolated"];
        if let Some(p) = profile {
            connect_args.push("--profile");
            connect_args.push(p);
        }
        connect_args.extend_from_slice(&["browser", "connect", ws_url]);

        let connect = actionbook()
            .env("HOME", home)
            .env("XDG_CONFIG_HOME", config_home)
            .env("XDG_DATA_HOME", data_home)
            .args(&connect_args)
            .timeout(Duration::from_secs(10))
            .output()
            .unwrap();
        let connect_ok = connect.status.success();

        let mut goto_args: Vec<&str> = vec!["--no-daemon", "--browser-mode", "isolated"];
        if let Some(p) = profile {
            goto_args.push("--profile");
            goto_args.push(p);
        }
        goto_args.extend_from_slice(&["browser", "goto", "https://example.com"]);

        let goto = actionbook()
            .env("HOME", home)
            .env("XDG_CONFIG_HOME", config_home)
            .env("XDG_DATA_HOME", data_home)
            .args(&goto_args)
            .timeout(Duration::from_secs(10))
            .output()
            .unwrap();
        let goto_ok = goto.status.success();

        let mut snap_args: Vec<&str> = vec!["--no-daemon", "--browser-mode", "isolated"];
        if let Some(p) = profile {
            snap_args.push("--profile");
            snap_args.push(p);
        }
        snap_args.extend_from_slice(&["browser", "snapshot"]);

        let snapshot = actionbook()
            .env("HOME", home)
            .env("XDG_CONFIG_HOME", config_home)
            .env("XDG_DATA_HOME", data_home)
            .args(&snap_args)
            .timeout(Duration::from_secs(10))
            .output()
            .unwrap();
        let snap_ok = snapshot.status.success();
        let snap_stdout = String::from_utf8_lossy(&snapshot.stdout).to_string();

        (connect_ok, goto_ok, snap_ok, snap_stdout)
    }

    /// Helper: run connect → open → snapshot against a mock CDP server.
    fn run_connect_open_snapshot(
        home: &str,
        config_home: &str,
        data_home: &str,
        ws_url: &str,
        profile: Option<&str>,
    ) -> (bool, bool, bool, String) {
        let mut connect_args: Vec<&str> =
            vec!["--no-daemon", "--json", "--browser-mode", "isolated"];
        if let Some(p) = profile {
            connect_args.push("--profile");
            connect_args.push(p);
        }
        connect_args.extend_from_slice(&["browser", "connect", ws_url]);

        let connect = actionbook()
            .env("HOME", home)
            .env("XDG_CONFIG_HOME", config_home)
            .env("XDG_DATA_HOME", data_home)
            .args(&connect_args)
            .timeout(Duration::from_secs(10))
            .output()
            .unwrap();
        let connect_ok = connect.status.success();

        let mut open_args: Vec<&str> = vec!["--no-daemon", "--browser-mode", "isolated"];
        if let Some(p) = profile {
            open_args.push("--profile");
            open_args.push(p);
        }
        open_args.extend_from_slice(&["browser", "open", "https://example.com"]);

        let open = actionbook()
            .env("HOME", home)
            .env("XDG_CONFIG_HOME", config_home)
            .env("XDG_DATA_HOME", data_home)
            .args(&open_args)
            .timeout(Duration::from_secs(10))
            .output()
            .unwrap();
        let open_ok = open.status.success();

        let mut snap_args: Vec<&str> = vec!["--no-daemon", "--browser-mode", "isolated"];
        if let Some(p) = profile {
            snap_args.push("--profile");
            snap_args.push(p);
        }
        snap_args.extend_from_slice(&["browser", "snapshot"]);

        let snapshot = actionbook()
            .env("HOME", home)
            .env("XDG_CONFIG_HOME", config_home)
            .env("XDG_DATA_HOME", data_home)
            .args(&snap_args)
            .timeout(Duration::from_secs(10))
            .output()
            .unwrap();
        let snap_ok = snapshot.status.success();
        let snap_stdout = String::from_utf8_lossy(&snapshot.stdout).to_string();

        (connect_ok, open_ok, snap_ok, snap_stdout)
    }

    // B.1 --no-daemon, default profile: connect → goto → snapshot

    #[test]
    #[serial]
    fn remote_no_daemon_default_profile_connect_goto_snapshot() {
        let (_tmp, home, config_home, data_home) = setup_config("team");
        let (port, _requests, server) = spawn_remote_cdp_server();
        let ws_url = format!("ws://127.0.0.1:{port}/automation");

        let (connect_ok, goto_ok, snap_ok, snap_stdout) =
            run_connect_goto_snapshot(&home, &config_home, &data_home, &ws_url, None);

        assert!(connect_ok, "connect should succeed");
        assert!(goto_ok, "goto should succeed");
        assert!(snap_ok, "snapshot should succeed");
        assert!(
            !snap_stdout.trim().is_empty(),
            "snapshot output should not be empty"
        );

        server.join().unwrap();
    }

    // B.2 --no-daemon, default profile: connect → open → snapshot

    #[test]
    #[serial]
    fn remote_no_daemon_default_profile_connect_open_snapshot() {
        let (_tmp, home, config_home, data_home) = setup_config("team");
        let (port, _requests, server) = spawn_remote_cdp_server();
        let ws_url = format!("ws://127.0.0.1:{port}/automation");

        let (connect_ok, open_ok, snap_ok, snap_stdout) =
            run_connect_open_snapshot(&home, &config_home, &data_home, &ws_url, None);

        assert!(connect_ok, "connect should succeed");
        assert!(open_ok, "open should succeed");
        assert!(snap_ok, "snapshot should succeed");
        assert!(
            !snap_stdout.trim().is_empty(),
            "snapshot output should not be empty"
        );

        server.join().unwrap();
    }

    // B.3 --no-daemon, custom profile: connect → goto → snapshot

    #[test]
    #[serial]
    fn remote_no_daemon_custom_profile_connect_goto_snapshot() {
        let (_tmp, home, config_home, data_home) = setup_config("team");
        let (port, _requests, server) = spawn_remote_cdp_server();
        let ws_url = format!("ws://127.0.0.1:{port}/automation");

        let (connect_ok, goto_ok, snap_ok, snap_stdout) = run_connect_goto_snapshot(
            &home,
            &config_home,
            &data_home,
            &ws_url,
            Some("custom-remote"),
        );

        assert!(connect_ok, "connect should succeed for custom profile");
        assert!(goto_ok, "goto should succeed for custom profile");
        assert!(snap_ok, "snapshot should succeed for custom profile");
        assert!(
            !snap_stdout.trim().is_empty(),
            "snapshot output should not be empty"
        );

        server.join().unwrap();
    }

    // B.4 --no-daemon, custom profile: connect → open → snapshot

    #[test]
    #[serial]
    fn remote_no_daemon_custom_profile_connect_open_snapshot() {
        let (_tmp, home, config_home, data_home) = setup_config("team");
        let (port, _requests, server) = spawn_remote_cdp_server();
        let ws_url = format!("ws://127.0.0.1:{port}/automation");

        let (connect_ok, open_ok, snap_ok, snap_stdout) = run_connect_open_snapshot(
            &home,
            &config_home,
            &data_home,
            &ws_url,
            Some("custom-remote"),
        );

        assert!(connect_ok, "connect should succeed for custom profile");
        assert!(open_ok, "open should succeed for custom profile");
        assert!(snap_ok, "snapshot should succeed for custom profile");
        assert!(
            !snap_stdout.trim().is_empty(),
            "snapshot output should not be empty"
        );

        server.join().unwrap();
    }

    // ── C. Concurrency: 3 independent sessions ────────────────────────

    #[test]
    #[serial]
    fn concurrent_three_sessions_connect_open_snapshot() {
        let profiles = ["concurrent-a", "concurrent-b", "concurrent-c"];

        // Spin up 3 independent mock CDP servers
        let servers: Vec<_> = profiles
            .iter()
            .map(|_| spawn_remote_cdp_server())
            .collect();

        // Shared isolated environment
        let (_tmp, home, config_home, data_home) = setup_config("team");

        // Connect all 3 profiles sequentially (writes session files)
        for (i, profile) in profiles.iter().enumerate() {
            let ws_url = format!("ws://127.0.0.1:{}/automation", servers[i].0);
            let connect = actionbook()
                .env("HOME", &home)
                .env("XDG_CONFIG_HOME", &config_home)
                .env("XDG_DATA_HOME", &data_home)
                .args([
                    "--no-daemon",
                    "--json",
                    "--browser-mode",
                    "isolated",
                    "--profile",
                    profile,
                    "browser",
                    "connect",
                    &ws_url,
                ])
                .timeout(Duration::from_secs(10))
                .output()
                .unwrap();
            assert!(
                connect.status.success(),
                "connect failed for {}: {}",
                profile,
                String::from_utf8_lossy(&connect.stderr)
            );
        }

        // Run open + snapshot concurrently for all 3 profiles
        let handles: Vec<_> = profiles
            .iter()
            .enumerate()
            .map(|(i, profile)| {
                let home = home.clone();
                let config_home = config_home.clone();
                let data_home = data_home.clone();
                let profile = profile.to_string();
                let _port = servers[i].0; // keep server alive

                std::thread::spawn(move || {
                    let open = actionbook()
                        .env("HOME", &home)
                        .env("XDG_CONFIG_HOME", &config_home)
                        .env("XDG_DATA_HOME", &data_home)
                        .args([
                            "--no-daemon",
                            "--browser-mode",
                            "isolated",
                            "--profile",
                            &profile,
                            "browser",
                            "open",
                            "https://example.com",
                        ])
                        .timeout(Duration::from_secs(10))
                        .output()
                        .unwrap();
                    assert!(
                        open.status.success(),
                        "[{}] open failed: {}",
                        profile,
                        String::from_utf8_lossy(&open.stderr)
                    );

                    let snapshot = actionbook()
                        .env("HOME", &home)
                        .env("XDG_CONFIG_HOME", &config_home)
                        .env("XDG_DATA_HOME", &data_home)
                        .args([
                            "--no-daemon",
                            "--browser-mode",
                            "isolated",
                            "--profile",
                            &profile,
                            "browser",
                            "snapshot",
                        ])
                        .timeout(Duration::from_secs(10))
                        .output()
                        .unwrap();
                    assert!(
                        snapshot.status.success(),
                        "[{}] snapshot failed: {}",
                        profile,
                        String::from_utf8_lossy(&snapshot.stderr)
                    );
                    let stdout = String::from_utf8_lossy(&snapshot.stdout).to_string();
                    assert!(
                        !stdout.trim().is_empty(),
                        "[{}] snapshot output should not be empty",
                        profile
                    );

                    profile
                })
            })
            .collect();

        // Collect results — all 3 threads must succeed
        let mut completed = Vec::new();
        for handle in handles {
            completed.push(handle.join().expect("thread panicked"));
        }
        assert_eq!(completed.len(), 3);

        // Clean up servers
        for (_, _, server) in servers {
            server.join().unwrap();
        }
    }

    // ── Cross-cutting flags ─────────────────────────────────────────

    #[test]
    fn extension_plus_profile_is_rejected() {
        actionbook()
            .args(["--extension", "--profile", "test", "browser", "status"])
            .timeout(std::time::Duration::from_secs(5))
            .assert()
            .failure()
            .stderr(
                predicate::str::is_match(
                    "(?i)--profile is not supported in extension mode|unexpected argument|not.*supported",
                )
                .unwrap(),
            );
    }
}

mod config_command {
    use super::*;

    #[test]
    fn config_requires_subcommand() {
        actionbook()
            .arg("config")
            .assert()
            .failure()
            .stderr(predicate::str::contains("subcommand"));
    }

    #[test]
    fn config_show_help() {
        actionbook()
            .args(["config", "show", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("configuration"));
    }

    #[test]
    fn config_set_requires_key_value() {
        actionbook()
            .args(["config", "set"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("KEY"));
    }

    #[test]
    fn config_get_requires_key() {
        actionbook()
            .args(["config", "get"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("KEY"));
    }

    #[test]
    fn config_edit_help() {
        actionbook()
            .args(["config", "edit", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());
    }

    #[test]
    fn config_show_runs() {
        actionbook().args(["config", "show"]).assert().success();
    }

    #[test]
    fn config_path_outputs_path() {
        actionbook()
            .args(["config", "path"])
            .assert()
            .success()
            .stdout(predicate::str::contains(".actionbook"));
    }

    #[test]
    fn config_path_json() {
        let (_tmp, home, config_home, data_home) = create_isolated_env();
        let output = actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["--json", "config", "path"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        let json: serde_json::Value =
            serde_json::from_str(stdout.trim()).expect("Should be valid JSON");
        assert!(json["path"].is_string());
    }

    #[test]
    fn config_set_rejects_unknown_key() {
        let (_tmp, home, config_home, data_home) = create_isolated_env();
        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["config", "set", "unknown.key", "value"])
            .assert()
            .failure();
    }

    #[test]
    fn config_get_rejects_unknown_key() {
        let (_tmp, home, config_home, data_home) = create_isolated_env();
        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["config", "get", "unknown.key"])
            .assert()
            .failure();
    }

    #[test]
    fn config_set_and_get_round_trip() {
        let (_tmp, home, config_home, data_home) = create_isolated_env();
        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args([
                "config",
                "set",
                "api.base_url",
                "https://custom.example.com",
            ])
            .assert()
            .success();

        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["config", "get", "api.base_url"])
            .assert()
            .success()
            .stdout(predicate::str::contains("https://custom.example.com"));
    }

    #[test]
    fn config_reset() {
        let (_tmp, home, config_home, data_home) = create_isolated_env();
        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["config", "reset"])
            .assert()
            .success();
    }
}

mod profile_command {
    use super::*;

    #[test]
    fn profile_requires_subcommand() {
        actionbook()
            .arg("profile")
            .assert()
            .failure()
            .stderr(predicate::str::contains("subcommand"));
    }

    #[test]
    fn profile_create_requires_name() {
        actionbook()
            .args(["profile", "create"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("NAME"));
    }

    #[test]
    fn profile_delete_requires_name() {
        actionbook()
            .args(["profile", "delete"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("NAME"));
    }

    #[test]
    fn profile_show_requires_name() {
        actionbook()
            .args(["profile", "show"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("NAME"));
    }

    #[test]
    fn profile_list_runs() {
        actionbook().args(["profile", "list"]).assert().success();
    }

    #[test]
    fn profile_list_json() {
        let (_tmp, home, config_home, data_home) = create_isolated_env();
        let output = actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["--json", "profile", "list"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        let _json: serde_json::Value =
            serde_json::from_str(stdout.trim()).expect("Should be valid JSON");
    }

    #[test]
    fn profile_create_and_show() {
        let (_tmp, home, config_home, data_home) = create_isolated_env();
        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["profile", "create", "test-profile", "--cdp-port", "9333"])
            .assert()
            .success();

        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["profile", "show", "test-profile"])
            .assert()
            .success()
            .stdout(predicate::str::contains("test-profile"));
    }

    #[test]
    fn profile_create_and_delete() {
        let (_tmp, home, config_home, data_home) = create_isolated_env();
        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["profile", "create", "delete-me"])
            .assert()
            .success();

        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["profile", "delete", "delete-me"])
            .assert()
            .success();

        // Verify deleted
        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["profile", "show", "delete-me"])
            .assert()
            .failure();
    }

    #[test]
    fn profile_show_fails_for_nonexistent() {
        let (_tmp, home, config_home, data_home) = create_isolated_env();
        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["profile", "show", "nonexistent-profile"])
            .assert()
            .failure();
    }

    #[test]
    fn profile_create_auto_assigns_cdp_port() {
        let (_tmp, home, config_home, data_home) = create_isolated_env();
        let output = actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["--json", "profile", "create", "auto-port"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        let json: serde_json::Value =
            serde_json::from_str(stdout.trim()).expect("Should be valid JSON");
        assert!(json["cdp_port"].is_number());
    }
}

mod extension_command {
    use super::*;

    #[test]
    fn extension_requires_subcommand() {
        actionbook()
            .arg("extension")
            .assert()
            .failure()
            .stderr(predicate::str::contains("subcommand"));
    }

    #[test]
    fn extension_status_help() {
        actionbook()
            .args(["extension", "status", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());
    }

    #[test]
    fn extension_ping_help() {
        actionbook()
            .args(["extension", "ping", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());
    }

    #[test]
    fn extension_install_help_shows_force() {
        actionbook()
            .args(["extension", "install", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--force"));
    }

    #[test]
    fn extension_stop_help() {
        actionbook()
            .args(["extension", "stop", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());
    }

    #[test]
    fn extension_uninstall_help() {
        actionbook()
            .args(["extension", "uninstall", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());
    }

    #[test]
    fn extension_path_outputs_path() {
        actionbook()
            .args(["extension", "path"])
            .timeout(std::time::Duration::from_secs(10))
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());
    }

    #[test]
    fn extension_path_json() {
        let output = actionbook()
            .args(["--json", "extension", "path"])
            .timeout(std::time::Duration::from_secs(10))
            .output()
            .unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        let _json: serde_json::Value =
            serde_json::from_str(stdout.trim()).expect("Should be valid JSON");
    }

    #[test]
    fn extension_status_reports_bridge_state() {
        let output = actionbook()
            .args(["extension", "status"])
            .timeout(std::time::Duration::from_secs(10))
            .output()
            .unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("running")
                || stdout.contains("Running")
                || stdout.contains("not running")
                || stdout.contains("Not running"),
            "Should report bridge state: {}",
            stdout
        );
    }

    #[test]
    fn extension_stop_when_not_running() {
        let (_tmp, home, config_home, data_home) = create_isolated_env();
        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["extension", "stop"])
            .timeout(std::time::Duration::from_secs(10))
            .assert()
            .success();
    }
}

mod setup_command {
    use super::*;

    #[test]
    fn setup_help_shows_all_options() {
        actionbook()
            .args(["setup", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--target"))
            .stdout(predicate::str::contains("--non-interactive"))
            .stdout(predicate::str::contains("--api-key"))
            .stdout(predicate::str::contains("--browser"))
            .stdout(predicate::str::contains("--reset"));
    }

    /// Helper: assert the setup command exits with 0 or 1 (not crash/signal).
    /// Uses a longer timeout since setup actually runs downloads/installs.
    fn assert_setup_exits_gracefully(args: &[&str]) {
        let (_tmp, home, config_home, data_home) = create_isolated_env();
        let output = actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(args)
            .timeout(std::time::Duration::from_secs(120))
            .output()
            .unwrap();
        let code = output.status.code().expect(
            "setup command was killed by signal (likely timed out); \
             it should exit within 120s instead of hanging",
        );
        assert!(
            code == 0 || code == 1,
            "Unexpected exit code: {}.\nstdout: {}\nstderr: {}",
            code,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    #[test]
    fn setup_non_interactive_runs() {
        assert_setup_exits_gracefully(&["setup", "--non-interactive", "--json"]);
    }

    #[test]
    fn setup_non_interactive_target_claude() {
        assert_setup_exits_gracefully(&[
            "setup",
            "--non-interactive",
            "--target",
            "claude",
            "--json",
        ]);
    }

    #[test]
    fn setup_with_api_key() {
        assert_setup_exits_gracefully(&[
            "setup",
            "--non-interactive",
            "--api-key",
            "test-key-12345",
            "--json",
        ]);
    }

    #[test]
    fn setup_browser_isolated() {
        assert_setup_exits_gracefully(&[
            "setup",
            "--non-interactive",
            "--browser",
            "isolated",
            "--json",
        ]);
    }

    #[test]
    fn setup_browser_extension() {
        assert_setup_exits_gracefully(&[
            "setup",
            "--non-interactive",
            "--browser",
            "extension",
            "--json",
        ]);
    }

    #[test]
    fn setup_reset() {
        assert_setup_exits_gracefully(&["setup", "--reset", "--non-interactive", "--json"]);
    }
}

mod global_flags {
    use super::*;

    #[test]
    fn json_flag_available_globally() {
        actionbook()
            .args(["--json", "search", "--help"])
            .assert()
            .success();
    }

    #[test]
    fn verbose_flag_available_globally() {
        actionbook()
            .args(["--verbose", "search", "--help"])
            .assert()
            .success();
    }

    #[test]
    fn headless_flag_available_globally() {
        actionbook()
            .args(["--headless", "search", "--help"])
            .assert()
            .success();
    }

    #[test]
    fn profile_flag_available_globally() {
        actionbook()
            .args(["--profile", "test", "search", "--help"])
            .assert()
            .success();
    }

    #[test]
    fn browser_path_flag_available_globally() {
        actionbook()
            .args(["--browser-path", "/usr/bin/chrome", "search", "--help"])
            .assert()
            .success();
    }

    #[test]
    fn cdp_flag_available_globally() {
        actionbook()
            .args(["--cdp", "9222", "search", "--help"])
            .assert()
            .success();
    }

    // ── --browser-mode option ───────────────────────────────────────

    #[test]
    fn browser_mode_accepts_isolated() {
        actionbook()
            .args(["--browser-mode", "isolated", "browser", "status"])
            .timeout(std::time::Duration::from_secs(10))
            .assert()
            .success();
    }

    #[test]
    fn browser_mode_accepts_extension() {
        actionbook()
            .args(["--browser-mode", "extension", "browser", "status"])
            .timeout(std::time::Duration::from_secs(10))
            .assert()
            .success();
    }

    #[test]
    fn browser_mode_rejects_invalid_value() {
        actionbook()
            .args(["--browser-mode", "invalid-mode", "browser", "status"])
            .timeout(std::time::Duration::from_secs(5))
            .assert()
            .failure();
    }

    // ── Stealth options ─────────────────────────────────────────────

    #[test]
    fn help_shows_stealth_options() {
        actionbook()
            .arg("--help")
            .assert()
            .success()
            .stdout(predicate::str::contains("--stealth"))
            .stdout(predicate::str::contains("--stealth-os"))
            .stdout(predicate::str::contains("--stealth-gpu"));
    }

    #[test]
    fn stealth_flag_accepted() {
        actionbook()
            .args(["--stealth", "browser", "status"])
            .timeout(std::time::Duration::from_secs(10))
            .assert()
            .success();
    }

    // ── Camofox options ─────────────────────────────────────────────

    #[test]
    fn help_shows_camofox_options() {
        actionbook()
            .arg("--help")
            .assert()
            .success()
            .stdout(predicate::str::contains("--camofox"))
            .stdout(predicate::str::contains("--camofox-port"));
    }
}

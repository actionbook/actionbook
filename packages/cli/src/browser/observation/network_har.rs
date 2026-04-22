//! `browser network har start` / `browser network har stop` commands.
//!
//! Records all network requests for a tab in HAR 1.2 format. Recording is
//! per-tab: multiple tabs can record independently at the same time.

use std::collections::HashSet;
use std::path::PathBuf;

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::cdp_session::{HarEntry, get_cdp_and_target};
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Default cap on per-response body size (bytes). Bodies larger than this are
/// dropped; metadata is still recorded. Keeps HAR output bounded even on long
/// recordings.
const DEFAULT_MAX_BODY_SIZE: usize = 5 * 1024 * 1024;
/// Default ring-buffer cap on number of entries. Oldest evicted when full.
const DEFAULT_MAX_ENTRIES: usize = 10000;

// ── Start ─────────────────────────────────────────────────────────────────────

/// Start HAR recording for a tab.
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser network har start --session s1 --tab t1
  actionbook browser network har start --session s1 --tab t1 \\
      --resource-types xhr,fetch,document --max-body-size 10485760

Captures HTTP requests/responses for the tab into a ring buffer. By default
only XHR and fetch requests are recorded, with response bodies fetched via
Network.getResponseBody (text bodies stored as-is, binary as base64).

Stop with `browser network har stop` to export a HAR 1.2 file.")]
pub struct StartCmd {
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
    /// Comma-separated CDP resource types to record. Case-insensitive.
    /// Valid values: document, stylesheet, image, media, font, script, texttrack,
    /// xhr, fetch, prefetch, eventsource, websocket, manifest, signedexchange,
    /// ping, cspviolationreport, preflight, other. Use "all" to record everything.
    #[arg(long, default_value = "xhr,fetch")]
    pub resource_types: String,
    /// Maximum number of entries to keep. Oldest are dropped when full.
    /// Set to 0 to disable the cap (unbounded memory use).
    #[arg(long, default_value_t = DEFAULT_MAX_ENTRIES)]
    pub max_entries: usize,
    /// Maximum bytes per response body; larger bodies drop the body text
    /// (metadata still recorded).
    #[arg(long, default_value_t = DEFAULT_MAX_BODY_SIZE)]
    pub max_body_size: usize,
    /// Skip fetching response bodies; only record request/response metadata.
    #[arg(long)]
    pub no_bodies: bool,
}

/// Parse a comma-separated resource-type list into the canonical CDP casing.
/// Returns empty set for "all" / "*" (= no filter). Any unknown token is a
/// hard error — silently dropping typos would turn "xrh" into "record
/// everything" from the caller's perspective.
fn parse_resource_types(s: &str) -> Result<HashSet<String>, Vec<String>> {
    let mut out = HashSet::new();
    let mut invalid = Vec::new();
    for tok in s.split(',').map(|t| t.trim()).filter(|t| !t.is_empty()) {
        let canonical = match tok.to_ascii_lowercase().as_str() {
            "all" | "*" => return Ok(HashSet::new()),
            "document" => "Document",
            "stylesheet" => "Stylesheet",
            "image" => "Image",
            "media" => "Media",
            "font" => "Font",
            "script" => "Script",
            "texttrack" => "TextTrack",
            "xhr" => "XHR",
            "fetch" => "Fetch",
            "prefetch" => "Prefetch",
            "eventsource" => "EventSource",
            "websocket" => "WebSocket",
            "manifest" => "Manifest",
            "signedexchange" => "SignedExchange",
            "ping" => "Ping",
            "cspviolationreport" => "CSPViolationReport",
            "preflight" => "Preflight",
            "other" => "Other",
            _ => {
                invalid.push(tok.to_string());
                continue;
            }
        };
        out.insert(canonical.to_string());
    }
    if !invalid.is_empty() {
        return Err(invalid);
    }
    Ok(out)
}

pub const START_COMMAND_NAME: &str = "browser network har start";

pub fn start_context(cmd: &StartCmd, result: &ActionResult) -> Option<ResponseContext> {
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
    Some(ResponseContext {
        session_id: cmd.session.clone(),
        tab_id,
        window_id: None,
        url: None,
        title: None,
    })
}

pub async fn execute_start(cmd: &StartCmd, registry: &SharedRegistry) -> ActionResult {
    let (cdp, target_id) = match get_cdp_and_target(registry, &cmd.session, &cmd.tab).await {
        Ok(pair) => pair,
        Err(e) => return e,
    };

    let cdp_session_id = match cdp.get_cdp_session_id(&target_id).await {
        Some(id) => id,
        None => {
            return ActionResult::fatal(
                "INTERNAL_ERROR",
                format!(
                    "no CDP session for tab '{}' (target {})",
                    cmd.tab, target_id
                ),
            );
        }
    };

    let resource_types = match parse_resource_types(&cmd.resource_types) {
        Ok(set) => set,
        Err(invalid) => {
            return ActionResult::fatal(
                "INVALID_RESOURCE_TYPES",
                format!(
                    "unknown resource type(s): {}. Valid values: all, document, stylesheet, image, media, font, script, texttrack, xhr, fetch, prefetch, eventsource, websocket, manifest, signedexchange, ping, cspviolationreport, preflight, other",
                    invalid.join(", ")
                ),
            );
        }
    };
    // Echo the canonical filter list back to the agent so it can tell at a
    // glance whether the alias ("all", "xhr,fetch") was expanded correctly.
    let resource_types_echo = if resource_types.is_empty() {
        "all".to_string()
    } else {
        let mut v: Vec<&str> = resource_types.iter().map(String::as_str).collect();
        v.sort_unstable();
        v.join(",")
    };

    match cdp
        .har_start(
            &cdp_session_id,
            &target_id,
            resource_types,
            cmd.max_entries,
            cmd.no_bodies,
            cmd.max_body_size,
        )
        .await
    {
        Ok(()) => ActionResult::ok(json!({
            "recording": true,
            "resource_types": resource_types_echo,
            "max_entries": cmd.max_entries,
            "max_body_size": cmd.max_body_size,
            "capture_bodies": !cmd.no_bodies,
            // Agents need to know where stop will write by default. The actual
            // filename is only decided at stop time (timestamped), so we
            // surface the directory — the stop response returns the full path.
            "output_dir": default_har_dir().to_string_lossy().as_ref(),
        })),
        Err("HAR_ALREADY_RECORDING") => ActionResult::fatal(
            "HAR_ALREADY_RECORDING",
            format!("HAR recording is already active for tab '{}'", cmd.tab),
        ),
        Err(other) => ActionResult::fatal("INTERNAL_ERROR", other.to_string()),
    }
}

// ── Stop ──────────────────────────────────────────────────────────────────────

/// Stop HAR recording and export to a file.
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser network har stop --session s1 --tab t1
  actionbook browser network har stop --session s1 --tab t1 --out /tmp/my.har
  actionbook browser network har stop --session s1 --tab t1 --out out.har

Stops recording and writes a HAR 1.2 JSON file. Returns { path, count,
dropped }. If --out is omitted, a timestamped file is created in
~/.actionbook/har/.

Relative --out paths are resolved against the CLI's current working
directory and the returned `path` is always absolute, so callers can
locate the file regardless of where the daemon was launched.")]
pub struct StopCmd {
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
    /// Output file path (default: ~/.actionbook/har/har-<timestamp>.har)
    #[arg(long)]
    pub out: Option<String>,
}

pub const STOP_COMMAND_NAME: &str = "browser network har stop";

pub fn stop_context(cmd: &StopCmd, result: &ActionResult) -> Option<ResponseContext> {
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
    Some(ResponseContext {
        session_id: cmd.session.clone(),
        tab_id,
        window_id: None,
        url: None,
        title: None,
    })
}

pub async fn execute_stop(cmd: &StopCmd, registry: &SharedRegistry) -> ActionResult {
    let (cdp, target_id) = match get_cdp_and_target(registry, &cmd.session, &cmd.tab).await {
        Ok(pair) => pair,
        Err(e) => return e,
    };

    let cdp_session_id = match cdp.get_cdp_session_id(&target_id).await {
        Some(id) => id,
        None => {
            return ActionResult::fatal(
                "INTERNAL_ERROR",
                format!(
                    "no CDP session for tab '{}' (target {})",
                    cmd.tab, target_id
                ),
            );
        }
    };

    // Peek at entries without removing the recorder yet.  The recorder is
    // only committed (removed) after the file has been written successfully,
    // so an I/O failure leaves the data intact and the user can retry.
    let (entries, dropped_count) = match cdp.har_stop(&cdp_session_id).await {
        Ok(v) => v,
        Err("HAR_NOT_RECORDING") => {
            return ActionResult::fatal(
                "HAR_NOT_RECORDING",
                format!("no HAR recording is active for tab '{}'", cmd.tab),
            );
        }
        Err(other) => return ActionResult::fatal("INTERNAL_ERROR", other.to_string()),
    };

    let count = entries.len();
    let out_path = match &cmd.out {
        Some(p) => PathBuf::from(p),
        None => default_har_path(),
    };
    // Resolve to an absolute path so the response is unambiguous regardless of
    // the daemon's CWD (daemon may have been spawned from a different dir than
    // the CLI invocation). std::path::absolute doesn't require the file to
    // exist yet, so run it before the write.
    let out_path = std::path::absolute(&out_path).unwrap_or(out_path);

    if let Some(parent) = out_path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return ActionResult::fatal(
            "IO_ERROR",
            format!("failed to create HAR output directory: {e}"),
        );
    }

    let har = serialize_har(entries, dropped_count);
    let har_str = match serde_json::to_string_pretty(&har) {
        Ok(s) => s,
        Err(e) => return ActionResult::fatal("IO_ERROR", format!("HAR serialization failed: {e}")),
    };

    if let Err(e) = std::fs::write(&out_path, har_str) {
        return ActionResult::fatal("IO_ERROR", format!("failed to write HAR file: {e}"));
    }

    // File written successfully — release the recorder from memory.
    cdp.har_commit(&cdp_session_id).await;

    ActionResult::ok(json!({
        "path": out_path.to_string_lossy().as_ref(),
        "count": count,
        "dropped": dropped_count,
    }))
}

// ── HAR 1.2 serialization ─────────────────────────────────────────────────────

fn serialize_har(entries: Vec<HarEntry>, dropped_count: usize) -> serde_json::Value {
    let entries_json: Vec<serde_json::Value> = entries.into_iter().map(har_entry_to_json).collect();
    let mut log = json!({
        "version": "1.2",
        "creator": {
            "name": "actionbook",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "entries": entries_json,
    });
    if dropped_count > 0 {
        // HAR 1.2 permits `_`-prefixed custom fields on any object.
        log["_droppedEntries"] = json!(dropped_count);
        log["_comment"] = json!(format!(
            "{dropped_count} earlier entries were dropped due to max_entries ring-buffer cap"
        ));
    }
    json!({ "log": log })
}

fn har_entry_to_json(e: HarEntry) -> serde_json::Value {
    let started_date_time = wall_time_to_rfc3339(e.wall_time);

    let req_headers: Vec<serde_json::Value> = e
        .request_headers
        .iter()
        .map(|(k, v)| json!({ "name": k, "value": v }))
        .collect();
    let request_cookies = e
        .request_headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("cookie"))
        .map(|(_, v)| parse_request_cookies(v))
        .unwrap_or_default();
    let query_string = parse_query_string(&e.url);

    let resp_headers: Vec<serde_json::Value> = e
        .response_headers
        .iter()
        .map(|(k, v)| json!({ "name": k, "value": v }))
        .collect();
    let resp_cookies: Vec<serde_json::Value> = e
        .response_headers
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("set-cookie"))
        .map(|(_, v)| {
            let name_value = v.split(';').next().unwrap_or("");
            let (name, value) = name_value.split_once('=').unwrap_or((name_value, ""));
            json!({ "name": name.trim(), "value": value.trim() })
        })
        .collect();

    let (timings, total_time) =
        compute_timings(e.cdp_timing.as_ref(), e.loading_finished_timestamp);

    let content_type = e
        .request_headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        .map(|(_, v)| v.as_str())
        .unwrap_or("text/plain")
        .to_string();

    let mut request = json!({
        "method": e.method,
        "url": e.url,
        "httpVersion": e.http_version,
        "cookies": request_cookies,
        "headers": req_headers,
        "queryString": query_string,
        "headersSize": -1,
        "bodySize": e.request_body_size,
    });
    if let Some(body) = e.post_data {
        request["postData"] = json!({ "mimeType": content_type, "text": body });
    }

    let mime_type = if e.mime_type.is_empty() {
        "application/octet-stream".to_string()
    } else {
        e.mime_type
    };

    let mut content = json!({
        "size": e.response_body_size,
        "mimeType": mime_type,
    });
    if let Some(body) = e.response_body {
        content["text"] = json!(body);
        if e.response_body_base64 {
            content["encoding"] = json!("base64");
        }
    }
    if let Some(dropped) = e.body_dropped_size_bytes {
        // `_`-prefixed fields are HAR 1.2-permitted extensions.
        content["_bodyDroppedSizeBytes"] = json!(dropped);
    }
    if let Some(err) = e.body_error {
        content["_bodyError"] = json!(err);
    }

    json!({
        "startedDateTime": started_date_time,
        "time": total_time,
        "request": request,
        "response": {
            "status": e.status.unwrap_or(0),
            "statusText": e.status_text,
            "httpVersion": e.http_version,
            "cookies": resp_cookies,
            "headers": resp_headers,
            "content": content,
            "redirectURL": e.redirect_url,
            "headersSize": -1,
            "bodySize": e.response_body_size,
        },
        "cache": {},
        "timings": timings,
        "_resourceType": e.resource_type,
    })
}

/// Compute HAR 1.2 timings from CDP ResourceTiming and loadingFinished timestamp.
/// All values in milliseconds; -1 means "not applicable" per HAR spec.
fn compute_timings(
    cdp_timing: Option<&serde_json::Value>,
    loading_finished_ts: Option<f64>,
) -> (serde_json::Value, f64) {
    let Some(t) = cdp_timing else {
        return (
            json!({ "blocked": -1, "dns": -1, "connect": -1, "ssl": -1, "send": 0, "wait": 0, "receive": 0 }),
            0.0,
        );
    };

    let get = |key: &str| t.get(key).and_then(|v| v.as_f64()).unwrap_or(-1.0);

    let request_time = get("requestTime");
    let dns_start = get("dnsStart");
    let dns_end = get("dnsEnd");
    let connect_start = get("connectStart");
    let connect_end = get("connectEnd");
    let ssl_start = get("sslStart");
    let ssl_end = get("sslEnd");
    let send_start = get("sendStart");
    let send_end = get("sendEnd");
    let recv_headers_start = get("receiveHeadersStart");
    let recv_headers_end = get("receiveHeadersEnd");

    let dns = if dns_start >= 0.0 && dns_end >= 0.0 {
        dns_end - dns_start
    } else {
        -1.0
    };
    let connect = if connect_start >= 0.0 && connect_end >= 0.0 {
        connect_end - connect_start
    } else {
        -1.0
    };
    let ssl = if ssl_start >= 0.0 && ssl_end >= 0.0 {
        ssl_end - ssl_start
    } else {
        -1.0
    };
    let send = if send_start >= 0.0 && send_end >= 0.0 {
        (send_end - send_start).max(0.0)
    } else {
        0.0
    };

    let wait_end = if recv_headers_start >= 0.0 {
        recv_headers_start
    } else {
        recv_headers_end
    };
    let wait = if send_end >= 0.0 && wait_end >= send_end {
        wait_end - send_end
    } else {
        0.0
    };

    let receive = loading_finished_ts
        .filter(|_| request_time >= 0.0 && recv_headers_end >= 0.0)
        .map(|lf_ts| {
            let recv_start_abs = request_time + recv_headers_end / 1000.0;
            ((lf_ts - recv_start_abs) * 1000.0).max(0.0)
        })
        .unwrap_or(0.0);

    let blocked = if dns_start > 0.0 {
        dns_start
    } else if connect_start > 0.0 {
        connect_start
    } else if send_start > 0.0 {
        send_start
    } else {
        -1.0
    };

    let total: f64 = [
        blocked.max(0.0),
        dns.max(0.0),
        connect.max(0.0),
        send,
        wait,
        receive,
    ]
    .iter()
    .sum();

    let timings = json!({
        "blocked": blocked,
        "dns": dns,
        "connect": connect,
        "ssl": ssl,
        "send": send,
        "wait": wait,
        "receive": receive,
    });

    (timings, total)
}

fn wall_time_to_rfc3339(wall_time: f64) -> String {
    let secs = if wall_time > 0.0 {
        wall_time.floor() as u64
    } else {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    };
    let millis = if wall_time > 0.0 {
        ((wall_time - wall_time.floor()) * 1000.0).round() as u64
    } else {
        0
    };
    unix_secs_to_rfc3339(secs, millis)
}

/// Format a Unix timestamp (seconds + milliseconds) as RFC 3339 / ISO 8601 UTC.
/// Output: `YYYY-MM-DDTHH:MM:SS.mmmZ`
fn unix_secs_to_rfc3339(secs: u64, millis: u64) -> String {
    // Days since Unix epoch → calendar date (Gregorian proleptic)
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let h = time_of_day / 3600;
    let m = (time_of_day % 3600) / 60;
    let s = time_of_day % 60;

    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        y, mo, d, h, m, s, millis
    )
}

fn parse_request_cookies(cookie_header: &str) -> Vec<serde_json::Value> {
    cookie_header
        .split(';')
        .filter_map(|pair| {
            let pair = pair.trim();
            if pair.is_empty() {
                return None;
            }
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            Some(json!({ "name": name.trim(), "value": value.trim() }))
        })
        .collect()
}

fn parse_query_string(url_str: &str) -> Vec<serde_json::Value> {
    let qs = url_str.find('?').map(|i| &url_str[i + 1..]).unwrap_or("");
    if qs.is_empty() {
        return Vec::new();
    }
    qs.split('&')
        .filter_map(|pair| {
            if pair.is_empty() {
                return None;
            }
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            let decode = |s: &str| s.replace('+', " ");
            Some(json!({ "name": decode(k), "value": decode(v) }))
        })
        .collect()
}

fn default_har_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(".actionbook")
        .join("har")
}

fn default_har_path() -> PathBuf {
    let dir = default_har_dir();
    let _ = std::fs::create_dir_all(&dir);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    dir.join(format!("har-{ts}.har"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    use clap::Parser;
    use futures_util::{SinkExt, StreamExt, stream::SplitSink};
    use serde_json::Value;
    use tokio::net::TcpListener;
    use tokio::sync::mpsc;
    use tokio_tungstenite::tungstenite::Message;

    use crate::action_result::ActionResult;
    use crate::cli::{BrowserCommands, Cli, Commands, HarCommands, NetworkCommands};
    use crate::daemon::cdp_session::CdpSession;
    use crate::daemon::registry::{SessionEntry, SessionState, new_shared_registry};
    use crate::types::{Mode, SessionId};

    type MockStream = tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>;
    type MockReader = futures_util::stream::SplitStream<MockStream>;
    type MockWriter = SplitSink<MockStream, Message>;

    async fn mock_ws_server() -> (String, mpsc::Receiver<(MockReader, MockWriter)>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        let url = format!("ws://127.0.0.1:{}", addr.port());

        let (tx, rx) = mpsc::channel(4);
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                let (writer, reader) = ws.split();
                if tx.send((reader, writer)).await.is_err() {
                    break;
                }
            }
        });

        (url, rx)
    }

    async fn read_json<S>(reader: &mut S) -> Value
    where
        S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
    {
        loop {
            let msg = reader.next().await.unwrap().unwrap();
            if let Message::Text(t) = msg {
                return serde_json::from_str(t.as_ref()).unwrap();
            }
        }
    }

    async fn send_json<S>(writer: &mut S, value: Value)
    where
        S: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
    {
        writer
            .send(Message::Text(value.to_string().into()))
            .await
            .unwrap();
    }

    fn parse_start_cmd_without_max_entries() -> StartCmd {
        let cli = Cli::try_parse_from([
            "actionbook",
            "browser",
            "network",
            "har",
            "start",
            "--session",
            "s1",
            "--tab",
            "t1",
        ])
        .expect("parse cli");

        match cli.command.expect("command") {
            Commands::Browser {
                command:
                    BrowserCommands::Network {
                        command:
                            NetworkCommands::Har {
                                command: HarCommands::Start(cmd),
                            },
                    },
            } => cmd,
            other => panic!("unexpected command tree: {other:?}"),
        }
    }

    async fn setup_extension_registry() -> (crate::daemon::registry::SharedRegistry, CdpSession, MockWriter) {
        let (url, mut conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();
        let (mut reader, mut writer) = conns.recv().await.unwrap();

        let registry = new_shared_registry();
        let mut entry = SessionEntry::starting(
            SessionId::new_unchecked("s1"),
            Mode::Extension,
            true,
            false,
            "profile-har-test".to_string(),
        );
        entry.status = SessionState::Running;
        entry.cdp = Some(cdp.clone());
        entry.push_tab("100".to_string(), "about:blank".to_string(), "Blank".to_string());
        registry.lock().await.insert(entry);

        let cdp_clone = cdp.clone();
        let register = tokio::spawn(async move {
            cdp_clone.register_extension_tab("100").await;
        });
        let enable = read_json(&mut reader).await;
        assert_eq!(enable["method"], "Network.enable");
        assert_eq!(enable["tabId"], 100);
        send_json(&mut writer, json!({"id": enable["id"], "result": {}})).await;
        register.await.unwrap();

        (registry, cdp, writer)
    }

    async fn send_request_will_be_sent(writer: &mut MockWriter, tab_id: u64, request_id: &str) {
        send_json(
            writer,
            json!({
                "method": "Network.requestWillBeSent",
                "tabId": tab_id,
                "params": {
                    "requestId": request_id,
                    "timestamp": 1.0,
                    "wallTime": 1_710_000_000.0,
                    "type": "XHR",
                    "request": {
                        "url": format!("https://example.test/api/{request_id}"),
                        "method": "GET",
                        "headers": {},
                    }
                }
            }),
        )
        .await;
    }

    #[test]
    fn parse_resource_types_known_tokens_canonicalize() {
        let set = parse_resource_types("xhr,fetch").unwrap();
        assert!(set.contains("XHR"));
        assert!(set.contains("Fetch"));
    }

    #[test]
    fn parse_resource_types_all_returns_empty_set() {
        assert!(parse_resource_types("all").unwrap().is_empty());
        assert!(parse_resource_types("*").unwrap().is_empty());
    }

    #[test]
    fn parse_resource_types_unknown_token_is_error() {
        // Regression: typo-only input used to silently become "record all".
        let err = parse_resource_types("xrh").unwrap_err();
        assert_eq!(err, vec!["xrh".to_string()]);
    }

    #[test]
    fn parse_resource_types_mixed_valid_invalid_is_error() {
        let err = parse_resource_types("xhr,bogus").unwrap_err();
        assert_eq!(err, vec!["bogus".to_string()]);
    }

    #[test]
    fn default_max_entries_is_10000() {
        assert_eq!(DEFAULT_MAX_ENTRIES, 10000);
    }

    #[tokio::test]
    async fn start_command_without_flag_uses_default_max_entries() {
        let cmd = parse_start_cmd_without_max_entries();
        let (registry, _cdp, _writer) = setup_extension_registry().await;
        let result = execute_start(&cmd, &registry).await;
        let data = match result {
            ActionResult::Ok { data } => data,
            other => panic!("expected ok start result, got {other:?}"),
        };
        assert_eq!(data["max_entries"], 10000);
    }

    #[tokio::test]
    async fn execute_stop_emits_truncated_marker_when_dropped_nonzero() {
        let (registry, cdp, mut writer) = setup_extension_registry().await;
        cdp.har_start(
            "tab:100",
            "100",
            HashSet::new(),
            10,
            true,
            DEFAULT_MAX_BODY_SIZE,
        )
        .await
        .expect("start har");

        for idx in 0..15 {
            send_request_will_be_sent(&mut writer, 100, &format!("req-{idx}")).await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let temp = tempfile::tempdir().expect("tempdir");
        let out = temp.path().join("truncated.har");
        let result = execute_stop(
            &StopCmd {
                session: "s1".to_string(),
                tab: "t1".to_string(),
                out: Some(out.to_string_lossy().to_string()),
            },
            &registry,
        )
        .await;
        let data = match result {
            ActionResult::Ok { data } => data,
            other => panic!("expected ok stop result, got {other:?}"),
        };

        assert_eq!(data["__truncated"], true);
        let warnings = data["__warnings"].as_array().expect("__warnings array");
        let warning = warnings
            .first()
            .and_then(|v| v.as_str())
            .expect("warning string");
        assert!(
            warning.starts_with("HAR_TRUNCATED: 5 earlier entries dropped (max_entries=10); "),
            "expected truncation warning prefix, got {warning:?}"
        );
        assert_eq!(data["max_entries"], 10);
        assert!(data["dropped"].as_u64().is_some_and(|v| v > 0));
    }

    #[tokio::test]
    async fn execute_stop_omits_truncated_marker_on_clean_stop() {
        let (registry, cdp, mut writer) = setup_extension_registry().await;
        cdp.har_start(
            "tab:100",
            "100",
            HashSet::new(),
            10,
            true,
            DEFAULT_MAX_BODY_SIZE,
        )
        .await
        .expect("start har");

        for idx in 0..3 {
            send_request_will_be_sent(&mut writer, 100, &format!("clean-{idx}")).await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let temp = tempfile::tempdir().expect("tempdir");
        let out = temp.path().join("clean.har");
        let result = execute_stop(
            &StopCmd {
                session: "s1".to_string(),
                tab: "t1".to_string(),
                out: Some(out.to_string_lossy().to_string()),
            },
            &registry,
        )
        .await;
        let data = match result {
            ActionResult::Ok { data } => data,
            other => panic!("expected ok stop result, got {other:?}"),
        };

        assert!(
            data.get("__truncated").is_none()
                || data["__truncated"].as_bool() == Some(false),
            "clean stop should not mark truncation: {data:?}"
        );
        assert!(
            data.get("__warnings").is_none()
                || data["__warnings"]
                    .as_array()
                    .is_some_and(|warnings| warnings.is_empty()),
            "clean stop should omit warnings: {data:?}"
        );
    }
}

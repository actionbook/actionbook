//! `browser network har start` / `browser network har stop` commands.
//!
//! Records all network requests for a tab in HAR 1.2 format. Recording is
//! per-tab: multiple tabs can record independently at the same time.

use std::path::PathBuf;

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::cdp_session::{HarEntry, get_cdp_and_target};
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

// ── Start ─────────────────────────────────────────────────────────────────────

/// Start HAR recording for a tab.
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser network har start --session s1 --tab t1

Starts capturing all HTTP requests/responses for the tab. Stop with
`browser network har stop` to export a HAR 1.2 file.")]
pub struct StartCmd {
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
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

    match cdp.har_start(&cdp_session_id).await {
        Ok(()) => ActionResult::ok(json!({ "recording": true })),
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

Stops recording and writes a HAR 1.2 JSON file. Returns { path, count }.
If --out is omitted, a timestamped file is created in ~/.actionbook/har/.")]
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
    let entries = match cdp.har_stop(&cdp_session_id).await {
        Ok(entries) => entries,
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

    if let Some(parent) = out_path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return ActionResult::fatal(
            "IO_ERROR",
            format!("failed to create HAR output directory: {e}"),
        );
    }

    let har = serialize_har(entries);
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
    }))
}

// ── HAR 1.2 serialization ─────────────────────────────────────────────────────

fn serialize_har(entries: Vec<HarEntry>) -> serde_json::Value {
    let entries_json: Vec<serde_json::Value> = entries.into_iter().map(har_entry_to_json).collect();
    json!({
        "log": {
            "version": "1.2",
            "creator": {
                "name": "actionbook",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "entries": entries_json,
        }
    })
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
            "content": {
                "size": e.response_body_size,
                "mimeType": mime_type,
            },
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

fn default_har_path() -> PathBuf {
    let dir = dirs::home_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(".actionbook")
        .join("har");
    let _ = std::fs::create_dir_all(&dir);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    dir.join(format!("har-{ts}.har"))
}

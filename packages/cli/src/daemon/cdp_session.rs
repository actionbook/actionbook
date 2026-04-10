//! Persistent per-session CDP connection with request multiplexing.
//!
//! One browser-level WebSocket connection per session. Commands target specific
//! tabs via CDP flat sessions (Target.attachToTarget + sessionId). Concurrent
//! requests are multiplexed using incrementing message IDs.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http;

use crate::error::CliError;

type PendingResponseTx = oneshot::Sender<Result<Value, CliError>>;
type PendingRequests = Arc<Mutex<HashMap<u64, PendingResponseTx>>>;
type EventSubs = Arc<Mutex<HashMap<String, Vec<mpsc::Sender<Value>>>>>;
/// Per-tab in-flight network request set, keyed by CDP flat-session ID.
/// Inner value: map of requestId → insertion timestamp (for stale cleanup).
/// Using a map (like Playwright's Set) instead of a counter avoids mismatches
/// when loadingFinished fires on a different CDP session (cross-origin iframes).
type TabNetPending = Arc<Mutex<HashMap<String, HashMap<String, std::time::Instant>>>>;
/// Cross-origin iframe frame_id → dedicated CDP session_id.
/// Populated by reader_loop from Target.attachedToTarget events.
type IframeSessions = Arc<Mutex<HashMap<String, String>>>;
/// Iframe session IDs that need DOM.enable + Accessibility.enable before use.
/// Populated by reader_loop; drained by callers (e.g. snapshot handler).
type PendingIframeEnables = Arc<Mutex<Vec<String>>>;

pub const MAX_TRACKED_REQUESTS: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackedRequest {
    pub request_id: String,
    pub url: String,
    pub method: String,
    pub resource_type: String,
    pub timestamp_ms: u64,
    pub status: Option<u16>,
    pub mime_type: Option<String>,
    pub request_headers: HashMap<String, String>,
    pub post_data: Option<String>,
    pub response_headers: HashMap<String, String>,
    /// Only populated by `network_request_detail` — not stored in the ring buffer.
    pub response_body: Option<String>,
}

type TabNetRequests = Arc<Mutex<HashMap<String, VecDeque<TrackedRequest>>>>;

#[derive(Debug, Clone, Default)]
pub struct NetworkRequestsFilter {
    pub url_substring: Option<String>,
    pub resource_types: Option<String>,
    pub method: Option<String>,
    pub status: Option<String>,
}

fn normalize_headers(headers: Option<&Value>) -> HashMap<String, String> {
    let Some(obj) = headers.and_then(|v| v.as_object()) else {
        return HashMap::new();
    };
    obj.iter()
        .filter_map(|(k, v)| v.as_str().map(|s| (k.to_lowercase(), s.to_string())))
        .collect()
}

fn record_request_will_be_sent(requests: &mut VecDeque<TrackedRequest>, params: &Value) {
    let request_id = params
        .get("requestId")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if request_id.is_empty() {
        return;
    }
    let req = params.get("request");
    let url = req
        .and_then(|r| r.get("url"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if url.starts_with("chrome://")
        || url.starts_with("chrome-untrusted://")
        || url.starts_with("chrome-extension://")
    {
        return;
    }
    let method = req
        .and_then(|r| r.get("method"))
        .and_then(|v| v.as_str())
        .unwrap_or("GET")
        .to_string();
    let resource_type = params
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("Other")
        .to_string();
    // CDP timestamp is seconds since epoch (float); convert to ms.
    let timestamp_ms = params
        .get("timestamp")
        .and_then(|v| v.as_f64())
        .map(|t| (t * 1000.0) as u64)
        .unwrap_or(0);
    let request_headers = normalize_headers(req.and_then(|r| r.get("headers")));
    let post_data = req
        .and_then(|r| r.get("postData"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    if requests.len() >= MAX_TRACKED_REQUESTS {
        requests.pop_front();
    }
    requests.push_back(TrackedRequest {
        request_id: request_id.to_string(),
        url,
        method,
        resource_type,
        timestamp_ms,
        status: None,
        mime_type: None,
        request_headers,
        post_data,
        response_headers: HashMap::new(),
        response_body: None,
    });
}

fn record_response_received(requests: &mut VecDeque<TrackedRequest>, params: &Value) {
    let request_id = params
        .get("requestId")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if request_id.is_empty() {
        return;
    }
    let response = params.get("response");
    let status = response
        .and_then(|r| r.get("status"))
        .and_then(|v| v.as_u64())
        .map(|s| s as u16);
    let mime_type = response
        .and_then(|r| r.get("mimeType"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let response_headers = normalize_headers(response.and_then(|r| r.get("headers")));

    if let Some(req) = requests
        .iter_mut()
        .rev()
        .find(|r| r.request_id == request_id)
    {
        req.status = status;
        req.mime_type = mime_type;
        req.response_headers = response_headers;
    }
}

fn matches_status_filter(status: Option<u16>, filter: &str) -> bool {
    let Some(s) = status else { return false };
    // Range: "400-499"
    if let Some((lo, hi)) = filter.split_once('-')
        && let (Ok(lo), Ok(hi)) = (lo.parse::<u16>(), hi.parse::<u16>())
    {
        return s >= lo && s <= hi;
    }
    // Class: "2xx", "4xx", etc.
    if filter.len() == 3
        && filter.ends_with("xx")
        && let Some(prefix) = filter.chars().next().and_then(|c| c.to_digit(10))
    {
        return (s / 100) as u32 == prefix;
    }
    // Exact: "200"
    if let Ok(code) = filter.parse::<u16>() {
        return s == code;
    }
    false
}

fn filter_tracked_requests(
    requests: &VecDeque<TrackedRequest>,
    filter: &NetworkRequestsFilter,
) -> Vec<TrackedRequest> {
    requests
        .iter()
        .filter(|req| {
            if let Some(ref sub) = filter.url_substring
                && !req.url.contains(sub.as_str())
            {
                return false;
            }
            if let Some(ref types) = filter.resource_types {
                let types_lower: Vec<String> =
                    types.split(',').map(|t| t.trim().to_lowercase()).collect();
                if !types_lower.contains(&req.resource_type.to_lowercase()) {
                    return false;
                }
            }
            if let Some(ref method) = filter.method
                && req.method.to_lowercase() != method.to_lowercase()
            {
                return false;
            }
            if let Some(ref status) = filter.status
                && !matches_status_filter(req.status, status)
            {
                return false;
            }
            true
        })
        .cloned()
        .collect()
}

fn clear_tracked_requests(requests: &mut VecDeque<TrackedRequest>) -> usize {
    let count = requests.len();
    requests.clear();
    count
}

fn tracked_request_detail(
    requests: &VecDeque<TrackedRequest>,
    request_id: &str,
) -> Option<TrackedRequest> {
    requests
        .iter()
        .rev()
        .find(|r| r.request_id == request_id)
        .cloned()
}

/// Persistent CDP connection for a single browser session.
///
/// All fields are `Arc`/`Sender` so `CdpSession` is cheaply `Clone`able.
/// The background reader task is spawned on `connect()` and routes incoming
/// WebSocket messages to the appropriate pending request by message ID.
#[derive(Clone)]
pub struct CdpSession {
    /// Channel to send raw WS text messages to the writer task.
    /// Wrapped in Option so `close()` can take it out, closing the channel
    /// and propagating shutdown to both reader and writer background tasks.
    writer_tx: Arc<Mutex<Option<mpsc::Sender<String>>>>,
    /// In-flight requests keyed by message ID.
    pending: PendingRequests,
    /// Atomic counter for generating unique message IDs.
    next_id: Arc<AtomicU64>,
    /// Mapping from CDP target_id → CDP sessionId (from Target.attachToTarget).
    tab_sessions: Arc<Mutex<HashMap<String, String>>>,
    /// Event subscribers keyed by `"{cdp_session_id}:{method}"`.
    event_subs: EventSubs,
    /// In-flight Network request count per CDP flat-session ID.
    /// Maintained by reader_loop from Network domain events; Network.enable is
    /// called in attach() so tracking starts before any user commands run.
    tab_net_pending: TabNetPending,
    /// Cross-origin iframe sessions discovered via Target.attachedToTarget events.
    /// Key: frame_id (= targetId from the event), Value: CDP session_id.
    iframe_sessions: IframeSessions,
    /// Iframe session IDs queued for domain enabling (DOM + Accessibility).
    /// reader_loop pushes here; callers drain before querying iframe AX trees.
    pending_iframe_enables: PendingIframeEnables,
    /// Per-tab ring buffer of tracked network requests, keyed by CDP session ID.
    /// Populated by reader_loop from Network events; capacity capped at MAX_TRACKED_REQUESTS.
    tab_net_requests: TabNetRequests,
}

impl CdpSession {
    /// Connect to a browser-level WebSocket endpoint and spawn background tasks.
    pub async fn connect(ws_url: &str) -> Result<Self, CliError> {
        Self::connect_with_headers(ws_url, &[]).await
    }

    /// Connect with custom headers (for cloud mode auth).
    pub async fn connect_with_headers(
        ws_url: &str,
        headers: &[(String, String)],
    ) -> Result<Self, CliError> {
        let mut request = ws_url
            .into_client_request()
            .map_err(|e| CliError::CdpConnectionFailed(format!("invalid WS URL: {e}")))?;

        for (key, value) in headers {
            let header_name = key.parse::<http::HeaderName>().map_err(|e| {
                CliError::InvalidArgument(format!("invalid header name '{key}': {e}"))
            })?;
            let header_value = http::HeaderValue::from_str(value).map_err(|e| {
                CliError::InvalidArgument(format!("invalid header value for '{key}': {e}"))
            })?;
            request.headers_mut().append(header_name, header_value);
        }

        let (ws, _) = connect_async(request)
            .await
            .map_err(|e| CliError::CdpConnectionFailed(e.to_string()))?;

        let (ws_writer, ws_reader) = ws.split();
        let pending: PendingRequests = Arc::new(Mutex::new(HashMap::new()));
        let next_id = Arc::new(AtomicU64::new(1));
        let (writer_tx, writer_rx) = mpsc::channel::<String>(64);
        let event_subs: EventSubs = Arc::new(Mutex::new(HashMap::new()));
        let tab_net_pending: TabNetPending = Arc::new(Mutex::new(HashMap::new()));
        let iframe_sessions: IframeSessions = Arc::new(Mutex::new(HashMap::new()));
        let pending_iframe_enables: PendingIframeEnables = Arc::new(Mutex::new(Vec::new()));
        let tab_sessions: Arc<Mutex<HashMap<String, String>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let tab_net_requests: TabNetRequests = Arc::new(Mutex::new(HashMap::new()));

        tokio::spawn(Self::writer_loop(ws_writer, writer_rx));
        tokio::spawn(Self::reader_loop(
            ws_reader,
            pending.clone(),
            event_subs.clone(),
            tab_net_pending.clone(),
            iframe_sessions.clone(),
            pending_iframe_enables.clone(),
            tab_sessions.clone(),
            tab_net_requests.clone(),
        ));

        Ok(CdpSession {
            writer_tx: Arc::new(Mutex::new(Some(writer_tx))),
            pending,
            next_id,
            tab_sessions,
            event_subs,
            tab_net_pending,
            iframe_sessions,
            pending_iframe_enables,
            tab_net_requests,
        })
    }

    /// Attach to a CDP target (tab) using flat session mode.
    ///
    /// Sends `Target.attachToTarget` with `flatten: true` and stores the
    /// returned `sessionId` for future `execute_on_tab` calls.
    /// Idempotent: if already attached, returns the existing sessionId.
    ///
    /// `user_agent`: if `Some`, stealth injection (Page.enable + script + UA override) is applied.
    /// Pass `None` to skip stealth (e.g. when stealth mode is disabled).
    pub async fn attach(
        &self,
        target_id: &str,
        user_agent: Option<&str>,
    ) -> Result<String, CliError> {
        // Check if already attached (idempotent).
        // However, if stealth UA is provided, we still need to apply stealth
        // to tabs that were auto-attached by Target.setAutoAttach.
        if let Some(existing) = self.tab_sessions.lock().await.get(target_id).cloned() {
            if user_agent.is_some() {
                // Tab already attached but stealth not yet applied.
                // Apply stealth to this existing session.
                self.apply_stealth(&existing, user_agent).await;
            }
            return Ok(existing);
        }

        let resp = self
            .execute(
                "Target.attachToTarget",
                json!({ "targetId": target_id, "flatten": true }),
                None,
            )
            .await?;

        let session_id = resp
            .get("result")
            .and_then(|r| r.get("sessionId"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                CliError::CdpError(format!(
                    "Target.attachToTarget did not return sessionId: {resp}"
                ))
            })?
            .to_string();

        self.tab_sessions
            .lock()
            .await
            .insert(target_id.to_string(), session_id.clone());

        // Enable the Network domain immediately so reader_loop tracks in-flight
        // requests from tab birth, before any user command (including wait network-idle)
        // is invoked.  Idempotent if called again on an already-enabled session.
        if let Err(e) = self
            .execute("Network.enable", json!({}), Some(&session_id))
            .await
        {
            // Roll back: remove local mapping AND detach from Chrome to avoid
            // an orphaned browser-side session that is unreachable from our map.
            self.tab_sessions.lock().await.remove(target_id);
            let _ = self
                .execute(
                    "Target.detachFromTarget",
                    json!({ "sessionId": session_id }),
                    None,
                )
                .await;
            return Err(e);
        }

        // Enable auto-attach for cross-origin iframe support.
        // Best-effort: some restricted DevTools endpoints may not support this,
        // and basic tab operation still works without it.
        let _ = self
            .execute(
                "Target.setAutoAttach",
                json!({
                    "autoAttach": true,
                    "waitForDebuggerOnStart": false,
                    "flatten": true
                }),
                Some(&session_id),
            )
            .await;

        // Apply stealth when user_agent is provided (stealth mode enabled).
        self.apply_stealth(&session_id, user_agent).await;

        Ok(session_id)
    }

    /// Apply stealth injection to a CDP session (if user_agent is Some).
    ///
    /// "Native-ish" strategy: inject minimal stealth JS (webdriver removal +
    /// automation marker cleanup + canvas noise) and strip "HeadlessChrome"
    /// from the User-Agent.  Does NOT override device metrics, plugins,
    /// screen size, language, or chrome.runtime — those stay real.
    async fn apply_stealth(&self, session_id: &str, user_agent: Option<&str>) {
        let ua = match user_agent {
            Some(ua) if !ua.is_empty() => ua,
            _ => return,
        };

        // Enable Page domain (required before addScriptToEvaluateOnNewDocument).
        let _ = self
            .execute("Page.enable", json!({}), Some(session_id))
            .await;

        // Inject stealth script so it runs at document start on every navigation.
        let stealth_source = &*crate::browser::stealth::STEALTH_JS;
        let _ = self
            .execute(
                "Page.addScriptToEvaluateOnNewDocument",
                json!({ "source": stealth_source }),
                Some(session_id),
            )
            .await;

        // Only override User-Agent to strip "HeadlessChrome" / "Headless".
        // Do NOT set acceptLanguage, platform, or userAgentMetadata —
        // let Chrome report real values to avoid fingerprint inconsistency.
        let _ = self
            .execute(
                "Emulation.setUserAgentOverride",
                json!({ "userAgent": ua }),
                Some(session_id),
            )
            .await;

        // Do NOT set Emulation.setDeviceMetricsOverride — let Chrome use
        // real screen dimensions.  Fixed 1920x1080 is a strong bot signal
        // (real users have 1366x768, 2560x1440, 3440x1440, etc.).
    }

    /// Register a tab for extension mode (no CDP flat-session handshake needed).
    ///
    /// The extension bridge relays CDP commands to a single attached tab and
    /// ignores the `sessionId` field entirely.  We insert an empty-string
    /// session ID so that `execute_on_tab` can look it up and the resulting
    /// WS message simply omits `sessionId` (because `execute()` only adds it
    /// when `Some`).  Actually we store `""` but `execute_on_tab` passes
    /// `Some("")` which adds `"sessionId":""` — the bridge ignores it.
    pub async fn register_extension_tab(&self, native_id: &str) {
        self.tab_sessions
            .lock()
            .await
            .insert(native_id.to_string(), String::new());
    }

    /// Detach from a CDP target (tab).
    pub async fn detach(&self, target_id: &str) -> Result<(), CliError> {
        let session_id = self
            .tab_sessions
            .lock()
            .await
            .remove(target_id)
            .ok_or_else(|| CliError::CdpError(format!("no session for target '{target_id}'")))?;

        self.execute(
            "Target.detachFromTarget",
            json!({ "sessionId": session_id }),
            None,
        )
        .await?;

        // Clean up the pending counter for this session.
        self.tab_net_pending.lock().await.remove(&session_id);

        // Clean up tracked network requests for this session.
        self.tab_net_requests.lock().await.remove(&session_id);

        // Clean up all event subscriptions for this session.
        self.unsubscribe_all(&session_id).await;

        Ok(())
    }

    /// Remove all event subscriptions for a given CDP session.
    pub async fn unsubscribe_all(&self, cdp_session_id: &str) {
        let prefix = format!("{cdp_session_id}:");
        self.event_subs
            .lock()
            .await
            .retain(|key, _| !key.starts_with(&prefix));
    }

    /// Execute a CDP command on a specific tab (by target_id).
    ///
    /// Looks up the CDP sessionId for the target and includes it in the message.
    pub async fn execute_on_tab(
        &self,
        target_id: &str,
        method: &str,
        params: Value,
    ) -> Result<Value, CliError> {
        let session_id = self
            .tab_sessions
            .lock()
            .await
            .get(target_id)
            .cloned()
            .ok_or_else(|| {
                CliError::CdpError(format!("no CDP session for target '{target_id}'"))
            })?;

        self.execute(method, params, Some(&session_id)).await
    }

    /// Execute a browser-level CDP command (no sessionId).
    pub async fn execute_browser(&self, method: &str, params: Value) -> Result<Value, CliError> {
        self.execute(method, params, None).await
    }

    /// Return the CDP flat-session ID for a target, or `None` if not attached.
    pub async fn get_cdp_session_id(&self, target_id: &str) -> Option<String> {
        self.tab_sessions.lock().await.get(target_id).cloned()
    }

    /// Return the current in-flight Network request count for a tab's CDP session.
    ///
    /// This counter is maintained by `reader_loop` from the moment `attach()` is
    /// called (which enables the Network domain), so it reflects ALL requests since
    /// tab attachment — not just those that started after `wait network-idle` was
    /// invoked.
    /// Returns the number of in-flight network requests for this session.
    /// Requests older than 10 seconds are considered stale (their
    /// loadingFinished likely fired on a different CDP session, e.g.
    /// cross-origin iframe) and are automatically evicted.
    pub async fn network_pending(&self, cdp_session_id: &str) -> i64 {
        let mut tp = self.tab_net_pending.lock().await;
        if let Some(map) = tp.get_mut(cdp_session_id) {
            let cutoff = std::time::Instant::now() - std::time::Duration::from_secs(3);
            map.retain(|_, ts| *ts > cutoff);
            map.len() as i64
        } else {
            0
        }
    }

    /// Return all tracked network requests for a tab's CDP session, applying optional filters.
    pub async fn network_requests(
        &self,
        cdp_session_id: &str,
        filter: &NetworkRequestsFilter,
    ) -> Vec<TrackedRequest> {
        let tnr = self.tab_net_requests.lock().await;
        if let Some(requests) = tnr.get(cdp_session_id) {
            filter_tracked_requests(requests, filter)
        } else {
            Vec::new()
        }
    }

    /// Return total count of tracked requests for a session (unfiltered).
    pub async fn network_requests_total(&self, cdp_session_id: &str) -> usize {
        self.tab_net_requests
            .lock()
            .await
            .get(cdp_session_id)
            .map(|q| q.len())
            .unwrap_or(0)
    }

    /// Clear all tracked network requests for a tab's CDP session. Returns cleared count.
    pub async fn clear_network_requests(&self, cdp_session_id: &str) -> usize {
        let mut tnr = self.tab_net_requests.lock().await;
        if let Some(requests) = tnr.get_mut(cdp_session_id) {
            clear_tracked_requests(requests)
        } else {
            0
        }
    }

    /// Return the detail entry for a single network request by request_id.
    pub async fn network_request_detail(
        &self,
        cdp_session_id: &str,
        request_id: &str,
    ) -> Option<TrackedRequest> {
        let tnr = self.tab_net_requests.lock().await;
        tnr.get(cdp_session_id)
            .and_then(|requests| tracked_request_detail(requests, request_id))
    }

    /// Subscribe to a CDP event for a specific flat-session.
    ///
    /// Returns a channel receiver that yields each matching event message.
    /// Subscribe BEFORE enabling the relevant CDP domain to avoid races.
    /// Dead receivers are removed lazily on the next event dispatch.
    pub async fn subscribe_events(
        &self,
        cdp_session_id: &str,
        method: &str,
    ) -> mpsc::Receiver<Value> {
        let key = format!("{cdp_session_id}:{method}");
        let (tx, rx) = mpsc::channel(256);
        self.event_subs
            .lock()
            .await
            .entry(key)
            .or_default()
            .push(tx);
        rx
    }

    /// Return a snapshot of the current iframe sessions (frame_id → cdp_session_id).
    pub async fn iframe_sessions(&self) -> HashMap<String, String> {
        self.iframe_sessions.lock().await.clone()
    }

    /// Drain iframe session IDs that need DOM.enable + Accessibility.enable.
    /// Called by snapshot handler before querying iframe AX trees.
    pub async fn drain_pending_iframe_enables(&self) -> Vec<String> {
        let mut pending = self.pending_iframe_enables.lock().await;
        std::mem::take(&mut *pending)
    }

    /// Clear all iframe sessions (used by session close/restart).
    pub async fn clear_iframe_sessions(&self) {
        self.iframe_sessions.lock().await.clear();
        self.pending_iframe_enables.lock().await.clear();
    }

    /// Gracefully shut down background reader/writer tasks and close the
    /// WebSocket connection. Idempotent — safe to call multiple times.
    ///
    /// Drops the writer channel sender, which causes the writer loop to exit,
    /// which closes the WS connection, which causes the reader loop to exit,
    /// which fails all pending requests with `SessionClosed`.
    pub async fn close(&self) {
        // Take and drop the writer sender — closes the channel.
        self.writer_tx.lock().await.take();

        // Fail all pending requests immediately instead of waiting for
        // the reader loop to notice the connection drop.
        let mut map = self.pending.lock().await;
        for (_, tx) in map.drain() {
            let _ = tx.send(Err(CliError::SessionClosed(
                "session was closed".to_string(),
            )));
        }
    }

    /// Low-level: send a CDP command and wait for its response.
    pub async fn execute(
        &self,
        method: &str,
        params: Value,
        session_id: Option<&str>,
    ) -> Result<Value, CliError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        let mut msg = json!({
            "id": id,
            "method": method,
            "params": params,
        });
        if let Some(sid) = session_id {
            msg["sessionId"] = json!(sid);
        }

        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);

        // Clone the sender (if still open) so we don't hold the mutex
        // across the potentially-blocking send().await.
        let writer = self.writer_tx.lock().await.clone();
        let send_result = match writer {
            Some(tx) => tx.send(msg.to_string()).await,
            None => Err(mpsc::error::SendError(msg.to_string())),
        };
        if send_result.is_err() {
            // Clean up pending entry to avoid leak
            self.pending.lock().await.remove(&id);
            return Err(CliError::SessionClosed(
                "session was closed while command was pending".to_string(),
            ));
        }

        // 60s covers slow operations (PDF, screenshot, large eval) while still
        // catching genuinely hung connections that the old code waited on forever.
        let resp = tokio::time::timeout(std::time::Duration::from_secs(60), rx)
            .await
            .map_err(|_| {
                // Clean up the pending entry on timeout to prevent leak.
                let pending = self.pending.clone();
                tokio::spawn(async move {
                    pending.lock().await.remove(&id);
                });
                CliError::Timeout
            })?
            .map_err(|_| CliError::CdpError("response channel dropped".to_string()))??;

        // Surface CDP-level errors (e.g., method not found, target crashed)
        if let Some(err) = resp.get("error") {
            let code = err.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
            let message = err
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown CDP error");
            return Err(CliError::CdpError(format!("CDP error {code}: {message}")));
        }

        Ok(resp)
    }

    /// Background task: read WS messages and route responses/events to callers.
    #[allow(clippy::too_many_arguments)]
    async fn reader_loop<S>(
        mut reader: S,
        pending: PendingRequests,
        event_subs: EventSubs,
        tab_net_pending: TabNetPending,
        iframe_sessions: IframeSessions,
        pending_iframe_enables: PendingIframeEnables,
        _tab_sessions: Arc<Mutex<HashMap<String, String>>>,
        tab_net_requests: TabNetRequests,
    ) where
        S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
    {
        while let Some(raw) = reader.next().await {
            let msg = match raw {
                Ok(Message::Text(t)) => t.to_string(),
                Ok(_) => continue,
                Err(_) => break,
            };

            let resp: Value = match serde_json::from_str(&msg) {
                Ok(v) => v,
                Err(_) => continue,
            };

            if let Some(id) = resp.get("id").and_then(|v| v.as_u64()) {
                // Response: route to the pending caller by message ID.
                let mut map = pending.lock().await;
                if let Some(tx) = map.remove(&id) {
                    let _ = tx.send(Ok(resp));
                }
            } else if let Some(method) = resp.get("method").and_then(|v| v.as_str()) {
                // Event: extract sessionId (empty string for browser-level events).
                let session_id = resp.get("sessionId").and_then(|v| v.as_str()).unwrap_or("");

                // Track cross-origin iframe sessions from Target.setAutoAttach.
                match method {
                    "Target.attachedToTarget" => {
                        if let Some(params) = resp.get("params") {
                            let target_type = params
                                .pointer("/targetInfo/type")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            if target_type == "iframe"
                                && let (Some(target_id), Some(sid)) = (
                                    params
                                        .pointer("/targetInfo/targetId")
                                        .and_then(|v| v.as_str()),
                                    params.get("sessionId").and_then(|v| v.as_str()),
                                )
                            {
                                iframe_sessions
                                    .lock()
                                    .await
                                    .insert(target_id.to_string(), sid.to_string());
                                pending_iframe_enables.lock().await.push(sid.to_string());
                            }
                        }
                    }
                    "Target.detachedFromTarget" => {
                        if let Some(sid) =
                            resp.pointer("/params/sessionId").and_then(|v| v.as_str())
                        {
                            iframe_sessions.lock().await.retain(|_, v| v != sid);
                        }
                    }
                    _ => {}
                }

                // Maintain per-tab in-flight request set (Playwright-style).
                // Using a Set<requestId> instead of a counter ensures that
                // cross-origin iframe requests (whose loadingFinished fires on
                // a child CDP session) don't permanently inflate the count.
                // Only track requests from the main frame (frameId == target_id)
                // to match Playwright's per-frame idle semantics.
                if !session_id.is_empty() {
                    match method {
                        "Network.requestWillBeSent" => {
                            let params = resp.get("params");
                            let req_type = params
                                .and_then(|p| p.get("type"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let url = params
                                .and_then(|p| p.pointer("/request/url"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let req_id = params
                                .and_then(|p| p.get("requestId"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            // Exclude request types that don't reliably fire
                            // loadingFinished on the same CDP session:
                            // - WebSocket/EventSource: persistent, never finish.
                            // - Favicon, data: URLs: Playwright compatibility.
                            // Requests from iframes whose loadingFinished arrives
                            // on a different CDP session are cleaned up by the
                            // stale eviction in network_pending().
                            let skip = req_type == "WebSocket"
                                || req_type == "EventSource"
                                || url.ends_with("/favicon.ico")
                                || url.starts_with("data:");
                            if !skip && !req_id.is_empty() {
                                let mut tp = tab_net_pending.lock().await;
                                tp.entry(session_id.to_string())
                                    .or_default()
                                    .insert(req_id.to_string(), std::time::Instant::now());
                            }
                            // Track request in ring buffer (all types, including skipped ones).
                            if let Some(params) = params {
                                let mut tnr = tab_net_requests.lock().await;
                                let requests = tnr.entry(session_id.to_string()).or_default();
                                record_request_will_be_sent(requests, params);
                            }
                        }
                        "Network.responseReceived" => {
                            if let Some(params) = resp.get("params") {
                                let mut tnr = tab_net_requests.lock().await;
                                if let Some(requests) = tnr.get_mut(session_id) {
                                    record_response_received(requests, params);
                                }
                            }
                        }
                        "Network.loadingFinished" | "Network.loadingFailed" => {
                            let req_id = resp
                                .pointer("/params/requestId")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            if !req_id.is_empty() {
                                let mut tp = tab_net_pending.lock().await;
                                if let Some(set) = tp.get_mut(session_id) {
                                    set.remove(req_id);
                                }
                            }
                        }
                        _ => {}
                    }
                }

                // Route to external event subscribers keyed by "{sessionId}:{method}".
                let key = format!("{session_id}:{method}");
                let mut subs = event_subs.lock().await;
                if let Some(txs) = subs.get_mut(&key) {
                    // try_send is non-blocking; retain removes closed receivers lazily.
                    txs.retain(|tx| tx.try_send(resp.clone()).is_ok());
                }
            }
        }

        // Connection dropped — fail all pending requests with SessionClosed.
        // cdp_error_to_result will further upgrade to CloudConnectionLost
        // for cloud sessions based on session mode.
        let mut map = pending.lock().await;
        for (_, tx) in map.drain() {
            let _ = tx.send(Err(CliError::SessionClosed(
                "session was closed while command was pending".to_string(),
            )));
        }

        // Also clear all event subscribers so their recv() returns None
        // instead of hanging forever. This unblocks waiters like goto's
        // Page.loadEventFired subscription.
        event_subs.lock().await.clear();
    }

    /// Background task: forward channel messages to WS writer.
    async fn writer_loop<S>(mut writer: S, mut rx: mpsc::Receiver<String>)
    where
        S: SinkExt<Message> + Unpin,
    {
        while let Some(text) = rx.recv().await {
            if writer.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    }
}

// ─── Helper ──────────────────────────────────────────────────────────

/// Extract CdpSession and native target_id from the registry.
///
/// `tab_id` is the short user-facing ID (e.g. "t1"); the returned target_id
/// is Chrome's native CDP target ID needed for `execute_on_tab`.
/// Returns `ActionResult` errors for SESSION_NOT_FOUND, TAB_NOT_FOUND,
/// or missing CDP connection.
pub async fn get_cdp_and_target(
    registry: &crate::daemon::registry::SharedRegistry,
    session_id: &str,
    tab_id: &str,
) -> Result<(CdpSession, String), crate::action_result::ActionResult> {
    let reg = registry.lock().await;
    let entry = reg.get(session_id).ok_or_else(|| {
        crate::action_result::ActionResult::fatal(
            "SESSION_NOT_FOUND",
            format!("session '{session_id}' not found"),
        )
    })?;
    let cdp = entry.cdp.clone().ok_or_else(|| {
        crate::action_result::ActionResult::fatal(
            "INTERNAL_ERROR",
            format!("no CDP connection for session '{session_id}'"),
        )
    })?;
    let native_id = entry
        .tabs
        .iter()
        .find(|t| t.id.0 == tab_id)
        .map(|t| t.native_id.clone())
        .ok_or_else(|| {
            crate::action_result::ActionResult::fatal(
                "TAB_NOT_FOUND",
                format!("tab '{tab_id}' not found"),
            )
        })?;
    Ok((cdp, native_id))
}

/// Convert a CliError from CDP operations into an ActionResult.
/// For cloud sessions, connection drops are surfaced as CLOUD_CONNECTION_LOST.
/// For local sessions, they use the default_code.
pub fn cdp_error_to_result(e: CliError, default_code: &str) -> crate::action_result::ActionResult {
    match &e {
        CliError::CloudConnectionLost(_) => crate::action_result::ActionResult::fatal_with_hint(
            "CLOUD_CONNECTION_LOST",
            e.to_string(),
            "cloud connection lost — retry or run `actionbook browser start --mode cloud ...` to reconnect",
        ),
        CliError::SessionClosed(_) => crate::action_result::ActionResult::fatal_with_hint(
            "SESSION_CLOSED",
            e.to_string(),
            "the session was closed while a command was still in flight — start a new session",
        ),
        _ => crate::action_result::ActionResult::fatal(default_code, e.to_string()),
    }
}

// ─── Unit Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream::SplitSink;
    use std::net::SocketAddr;
    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::Message;

    /// Start a mock WebSocket server. Returns the URL and a channel that
    /// yields each accepted connection's (reader, writer) pair.
    async fn mock_ws_server() -> (
        String,
        mpsc::Receiver<(
            futures_util::stream::SplitStream<
                tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
            >,
            SplitSink<tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>, Message>,
        )>,
    ) {
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

    /// Helper: read one JSON message from the mock server's reader.
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

    /// Helper: send a JSON response from the mock server.
    async fn send_json<S>(writer: &mut S, value: Value)
    where
        S: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
    {
        writer
            .send(Message::Text(value.to_string().into()))
            .await
            .unwrap();
    }

    // ── 1. test_message_id_increment ─────────────────────────────────

    #[tokio::test]
    async fn test_message_id_increment() {
        let (url, mut conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();
        let (mut reader, mut writer) = conns.recv().await.unwrap();

        // Send 3 sequential requests, verify IDs are 1, 2, 3
        for expected_id in 1..=3u64 {
            let cdp = cdp.clone();
            let method = format!("Test.method{expected_id}");
            let handle = tokio::spawn(async move { cdp.execute(&method, json!({}), None).await });

            let msg = read_json(&mut reader).await;
            assert_eq!(msg["id"], expected_id, "message id should be {expected_id}");
            assert_eq!(msg["method"], format!("Test.method{expected_id}"));
            assert!(
                msg.get("sessionId").is_none(),
                "no sessionId for browser-level"
            );

            // Reply
            send_json(&mut writer, json!({"id": expected_id, "result": {}})).await;
            handle.await.unwrap().unwrap();
        }
    }

    // ── 2. test_request_response_matching ─────────────────────────────

    #[tokio::test]
    async fn test_request_response_matching() {
        let (url, mut conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();
        let (mut reader, mut writer) = conns.recv().await.unwrap();

        // Send 2 requests concurrently
        let cdp1 = cdp.clone();
        let h1 = tokio::spawn(async move { cdp1.execute("Method.A", json!({}), None).await });
        let cdp2 = cdp.clone();
        let h2 = tokio::spawn(async move { cdp2.execute("Method.B", json!({}), None).await });

        // Read both requests
        let msg1 = read_json(&mut reader).await;
        let msg2 = read_json(&mut reader).await;
        let id1 = msg1["id"].as_u64().unwrap();
        let id2 = msg2["id"].as_u64().unwrap();

        // Reply in REVERSE order (id2 first, then id1)
        send_json(&mut writer, json!({"id": id2, "result": {"value": "B"}})).await;
        send_json(&mut writer, json!({"id": id1, "result": {"value": "A"}})).await;

        // Each caller gets the correct response
        let r1 = h1.await.unwrap().unwrap();
        let r2 = h2.await.unwrap().unwrap();
        assert_eq!(r1["result"]["value"], "A");
        assert_eq!(r2["result"]["value"], "B");
    }

    // ── 3. test_attach_detach ─────────────────────────────────────────

    #[tokio::test]
    async fn test_attach_detach() {
        let (url, mut conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();
        let (mut reader, mut writer) = conns.recv().await.unwrap();

        // Attach
        let cdp_clone = cdp.clone();
        let attach_handle = tokio::spawn(async move { cdp_clone.attach("TARGET_ABC", None).await });

        let msg = read_json(&mut reader).await;
        assert_eq!(msg["method"], "Target.attachToTarget");
        assert_eq!(msg["params"]["targetId"], "TARGET_ABC");
        assert_eq!(msg["params"]["flatten"], true);

        send_json(
            &mut writer,
            json!({"id": msg["id"], "result": {"sessionId": "CDP_SESS_1"}}),
        )
        .await;

        // attach() enables Network domain immediately after storing the session.
        let net_msg = read_json(&mut reader).await;
        assert_eq!(net_msg["method"], "Network.enable");
        assert_eq!(net_msg["sessionId"], "CDP_SESS_1");
        send_json(&mut writer, json!({"id": net_msg["id"], "result": {}})).await;

        // attach() then calls Target.setAutoAttach for iframe support.
        let auto_attach_msg = read_json(&mut reader).await;
        assert_eq!(auto_attach_msg["method"], "Target.setAutoAttach");
        assert_eq!(auto_attach_msg["sessionId"], "CDP_SESS_1");
        assert_eq!(auto_attach_msg["params"]["autoAttach"], true);
        assert_eq!(auto_attach_msg["params"]["flatten"], true);
        send_json(
            &mut writer,
            json!({"id": auto_attach_msg["id"], "result": {}}),
        )
        .await;

        let session_id = attach_handle.await.unwrap().unwrap();
        assert_eq!(session_id, "CDP_SESS_1");

        // execute_on_tab should include the sessionId
        let cdp_clone = cdp.clone();
        let exec_handle = tokio::spawn(async move {
            cdp_clone
                .execute_on_tab(
                    "TARGET_ABC",
                    "Runtime.evaluate",
                    json!({"expression": "1+1"}),
                )
                .await
        });

        let msg = read_json(&mut reader).await;
        assert_eq!(msg["sessionId"], "CDP_SESS_1");
        assert_eq!(msg["method"], "Runtime.evaluate");
        send_json(
            &mut writer,
            json!({"id": msg["id"], "result": {"result": {"value": 2}}}),
        )
        .await;
        exec_handle.await.unwrap().unwrap();

        // Detach
        let cdp_clone = cdp.clone();
        let detach_handle = tokio::spawn(async move { cdp_clone.detach("TARGET_ABC").await });

        let msg = read_json(&mut reader).await;
        assert_eq!(msg["method"], "Target.detachFromTarget");
        assert_eq!(msg["params"]["sessionId"], "CDP_SESS_1");
        send_json(&mut writer, json!({"id": msg["id"], "result": {}})).await;
        detach_handle.await.unwrap().unwrap();

        // After detach, execute_on_tab should fail
        let result = cdp.execute_on_tab("TARGET_ABC", "Test", json!({})).await;
        assert!(result.is_err());
    }

    // ── 4. test_concurrent_requests ───────────────────────────────────

    #[tokio::test]
    async fn test_concurrent_requests() {
        let (url, mut conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();
        let (mut reader, mut writer) = conns.recv().await.unwrap();

        // Pre-populate tab_sessions for 3 tabs (skip attach handshake)
        {
            let mut sessions = cdp.tab_sessions.lock().await;
            sessions.insert("T1".to_string(), "S1".to_string());
            sessions.insert("T2".to_string(), "S2".to_string());
            sessions.insert("T3".to_string(), "S3".to_string());
        }

        // Spawn 3 concurrent requests
        let handles: Vec<_> = ["T1", "T2", "T3"]
            .iter()
            .map(|tid| {
                let cdp = cdp.clone();
                let tid = tid.to_string();
                tokio::spawn(async move {
                    cdp.execute_on_tab(&tid, "Runtime.evaluate", json!({"expression": "1"}))
                        .await
                })
            })
            .collect();

        // Read all 3 requests from mock server
        let mut requests = Vec::new();
        for _ in 0..3 {
            requests.push(read_json(&mut reader).await);
        }

        // Verify each has a unique id and correct sessionId
        let ids: Vec<u64> = requests.iter().map(|r| r["id"].as_u64().unwrap()).collect();
        assert_eq!(ids.len(), 3);
        assert!(
            ids[0] != ids[1] && ids[1] != ids[2] && ids[0] != ids[2],
            "IDs must be unique"
        );

        let session_ids: Vec<&str> = requests
            .iter()
            .map(|r| r["sessionId"].as_str().unwrap())
            .collect();
        assert!(session_ids.contains(&"S1"));
        assert!(session_ids.contains(&"S2"));
        assert!(session_ids.contains(&"S3"));

        // Reply to all 3
        for req in &requests {
            let id = req["id"].as_u64().unwrap();
            let sid = req["sessionId"].as_str().unwrap();
            send_json(&mut writer, json!({"id": id, "result": {"value": sid}})).await;
        }

        // Verify each handle resolves with correct value
        for handle in handles {
            let resp = handle.await.unwrap().unwrap();
            assert!(resp["result"]["value"].is_string());
        }
    }

    // ── 5. test_connection_drop ───────────────────────────────────────

    #[tokio::test]
    async fn test_connection_drop() {
        let (url, mut conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();
        let (reader, writer) = conns.recv().await.unwrap();

        // Start a request, then drop the full server-side connection
        let cdp_clone = cdp.clone();
        let handle =
            tokio::spawn(async move { cdp_clone.execute("Test.method", json!({}), None).await });

        // Give a moment for the request to be sent
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Drop both sides → fully closes the WS connection
        drop(reader);
        drop(writer);

        // Caller should get an error, not hang forever
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), handle)
            .await
            .expect("should not timeout")
            .unwrap();

        assert!(result.is_err(), "should return error when connection drops");
    }

    // ── 6. test_execute_on_unknown_tab ────────────────────────────────

    #[tokio::test]
    async fn test_execute_on_unknown_tab() {
        let (url, mut _conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();

        // execute_on_tab with non-attached target should fail immediately
        let result = cdp
            .execute_on_tab("UNKNOWN_TARGET", "Runtime.evaluate", json!({}))
            .await;

        assert!(result.is_err(), "should fail for unknown target");
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("no CDP session for target"),
            "error should mention missing session, got: {err}"
        );
    }

    // ── 7. test_network_pending_counter ──────────────────────────────

    #[tokio::test]
    async fn test_network_pending_counter() {
        let (url, mut conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();
        let (_, mut writer) = conns.recv().await.unwrap();

        // Pre-populate a tab session (simulates attach having stored the session).
        cdp.tab_sessions
            .lock()
            .await
            .insert("T_NET".to_string(), "S_NET".to_string());

        // Initially 0
        assert_eq!(cdp.network_pending("S_NET").await, 0);

        // Simulate Network.requestWillBeSent event (no id field)
        send_json(
            &mut writer,
            json!({
                "method": "Network.requestWillBeSent",
                "sessionId": "S_NET",
                "params": { "requestId": "r1", "frameId": "T_NET" }
            }),
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(cdp.network_pending("S_NET").await, 1);

        // Second request
        send_json(
            &mut writer,
            json!({
                "method": "Network.requestWillBeSent",
                "sessionId": "S_NET",
                "params": { "requestId": "r2", "frameId": "T_NET" }
            }),
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(cdp.network_pending("S_NET").await, 2);

        // First finishes
        send_json(
            &mut writer,
            json!({
                "method": "Network.loadingFinished",
                "sessionId": "S_NET",
                "params": { "requestId": "r1", "frameId": "T_NET" }
            }),
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(cdp.network_pending("S_NET").await, 1);

        // Second fails
        send_json(
            &mut writer,
            json!({
                "method": "Network.loadingFailed",
                "sessionId": "S_NET",
                "params": { "requestId": "r2", "frameId": "T_NET" }
            }),
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(cdp.network_pending("S_NET").await, 0);

        // Does not go negative on extra terminal events
        send_json(
            &mut writer,
            json!({
                "method": "Network.loadingFinished",
                "sessionId": "S_NET",
                "params": { "requestId": "r_unknown" }
            }),
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(cdp.network_pending("S_NET").await, 0);
    }

    // ── 8b. test_network_counter_skips_websocket_favicon_data ─────────

    /// WebSocket, favicon, and data: requests must not increment the pending
    /// counter.  Their loadingFinished/loadingFailed must also be suppressed
    /// so they don't undercount other in-flight requests.
    #[tokio::test]
    async fn test_network_counter_skips_websocket_favicon_data() {
        let (url, mut conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();
        let (_, mut writer) = conns.recv().await.unwrap();

        cdp.tab_sessions
            .lock()
            .await
            .insert("T_SKIP".to_string(), "S_SKIP".to_string());

        // ── WebSocket request: should be skipped ──
        send_json(
            &mut writer,
            json!({
                "method": "Network.requestWillBeSent",
                "sessionId": "S_SKIP",
                "params": {
                    "requestId": "ws1",
                    "type": "WebSocket",
                    "request": { "url": "wss://example.com/socket" }
                }
            }),
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(
            cdp.network_pending("S_SKIP").await,
            0,
            "WebSocket request must not increment counter"
        );

        // ── Favicon request: should be skipped ──
        send_json(
            &mut writer,
            json!({
                "method": "Network.requestWillBeSent",
                "sessionId": "S_SKIP",
                "params": {
                    "requestId": "fav1",
                    "request": { "url": "https://example.com/favicon.ico" }
                }
            }),
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(
            cdp.network_pending("S_SKIP").await,
            0,
            "favicon request must not increment counter"
        );

        // ── data: URL request: should be skipped ──
        send_json(
            &mut writer,
            json!({
                "method": "Network.requestWillBeSent",
                "sessionId": "S_SKIP",
                "params": {
                    "requestId": "data1",
                    "request": { "url": "data:image/png;base64,iVBOR..." }
                }
            }),
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(
            cdp.network_pending("S_SKIP").await,
            0,
            "data: URL must not increment counter"
        );

        // ── Normal request: should be counted ──
        send_json(
            &mut writer,
            json!({
                "method": "Network.requestWillBeSent",
                "sessionId": "S_SKIP",
                "params": {
                    "requestId": "r1",
                    "frameId": "T_SKIP",
                    "request": { "url": "https://example.com/api/data" }
                }
            }),
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(
            cdp.network_pending("S_SKIP").await,
            1,
            "normal request must increment counter"
        );

        // ── Favicon loadingFinished must NOT undercount ──
        // (This is the P0 bug that Codex bot caught)
        send_json(
            &mut writer,
            json!({
                "method": "Network.loadingFinished",
                "sessionId": "S_SKIP",
                "params": { "requestId": "fav1" }
            }),
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(
            cdp.network_pending("S_SKIP").await,
            1,
            "favicon finish must not decrement — its +1 was skipped"
        );

        // ── data: loadingFinished must NOT undercount ──
        send_json(
            &mut writer,
            json!({
                "method": "Network.loadingFinished",
                "sessionId": "S_SKIP",
                "params": { "requestId": "data1" }
            }),
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(
            cdp.network_pending("S_SKIP").await,
            1,
            "data: finish must not decrement — its +1 was skipped"
        );

        // ── Normal request finishes: counter should go to 0 ──
        send_json(
            &mut writer,
            json!({
                "method": "Network.loadingFinished",
                "sessionId": "S_SKIP",
                "params": { "requestId": "r1", "frameId": "T_NET" }
            }),
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(
            cdp.network_pending("S_SKIP").await,
            0,
            "normal request finish should bring counter to 0"
        );
    }

    // ── 9. test_event_routing ─────────────────────────────────────────

    #[tokio::test]
    async fn test_event_routing() {
        let (url, mut conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();
        let (_, mut writer) = conns.recv().await.unwrap();

        // Pre-populate a tab session to simulate attach
        cdp.tab_sessions
            .lock()
            .await
            .insert("TARGET_EV".to_string(), "SESSION_EV".to_string());

        // Subscribe before the event arrives
        let mut rx = cdp
            .subscribe_events("SESSION_EV", "Network.requestWillBeSent")
            .await;

        // Server sends a CDP event (no id field)
        send_json(
            &mut writer,
            json!({
                "method": "Network.requestWillBeSent",
                "sessionId": "SESSION_EV",
                "params": { "requestId": "req-42" }
            }),
        )
        .await;

        let event = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("timed out waiting for event")
            .expect("channel closed");

        assert_eq!(event["method"], "Network.requestWillBeSent");
        assert_eq!(event["params"]["requestId"], "req-42");
    }

    // ── 10. test_event_not_routed_to_wrong_session ────────────────────

    #[tokio::test]
    async fn test_event_not_routed_to_wrong_session() {
        let (url, mut conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();
        let (_, mut writer) = conns.recv().await.unwrap();

        // Subscribe to SESSION_A events
        let mut rx_a = cdp
            .subscribe_events("SESSION_A", "Network.requestWillBeSent")
            .await;

        // Send event for SESSION_B (different session)
        send_json(
            &mut writer,
            json!({
                "method": "Network.requestWillBeSent",
                "sessionId": "SESSION_B",
                "params": { "requestId": "req-99" }
            }),
        )
        .await;

        // Allow reader loop to process
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // SESSION_A subscriber must NOT receive SESSION_B event
        assert!(
            rx_a.try_recv().is_err(),
            "should not receive event destined for a different session"
        );
    }

    // ── 11. test_close_stops_background_tasks ────────────────────────

    #[tokio::test]
    async fn test_close_stops_background_tasks() {
        let (url, mut conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();
        let (_reader, _writer) = conns.recv().await.unwrap();

        // close() should terminate reader/writer tasks
        cdp.close().await;

        // After close, sending a command should fail (not hang).
        // Use a timeout to prevent infinite hang if close() is a no-op.
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            cdp.execute("Test.method", json!({}), None),
        )
        .await;

        match result {
            Ok(Err(_)) => {} // Expected: execute returns error immediately
            Ok(Ok(_)) => panic!("execute after close() should fail, not succeed"),
            Err(_) => {
                panic!("execute after close() hung — close() did not shut down background tasks")
            }
        }
    }

    // ── 12. test_close_idempotent ────────────────────────────────────

    #[tokio::test]
    async fn test_close_idempotent() {
        let (url, mut conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();
        let (_reader, _writer) = conns.recv().await.unwrap();

        // Calling close() twice should not panic
        cdp.close().await;
        cdp.close().await;

        // And execute should still fail after double close
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            cdp.execute("Test.method", json!({}), None),
        )
        .await;
        assert!(
            matches!(result, Ok(Err(_))),
            "execute after double close() should fail"
        );
    }

    // ── 13. test_execute_timeout ─────────────────────────────────────

    /// execute() must return CliError::Timeout when server never replies
    /// (instead of hanging forever). Uses tokio::time::pause() so we don't
    /// actually wait 30 seconds.
    #[tokio::test(start_paused = true)]
    async fn test_execute_timeout() {
        let (url, mut conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();
        let (_reader, _writer) = conns.recv().await.unwrap();

        // Server never replies — execute should timeout after 30s
        let result = cdp.execute("Test.noReply", json!({}), None).await;

        assert!(result.is_err(), "should timeout, not hang");
        let err = result.unwrap_err();
        assert!(
            matches!(err, CliError::Timeout),
            "expected Timeout, got: {err}"
        );
    }

    // ── 14. test_execute_timeout_cleans_pending ──────────────────────

    /// After timeout, the pending map entry for the timed-out request must
    /// be cleaned up to prevent memory leaks.
    #[tokio::test(start_paused = true)]
    async fn test_execute_timeout_cleans_pending() {
        let (url, mut conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();
        let (_reader, _writer) = conns.recv().await.unwrap();

        // execute will timeout
        let _ = cdp.execute("Test.noReply", json!({}), None).await;

        // Give the spawn cleanup task a tick to run
        tokio::task::yield_now().await;

        let map = cdp.pending.lock().await;
        assert!(
            map.is_empty(),
            "pending map should be empty after timeout, has {} entries",
            map.len()
        );
    }

    // ── 15. test_attach_propagates_network_enable_error ──────────────

    /// When Network.enable returns a CDP error during attach(), attach()
    /// must propagate the error (not silently swallow it).
    #[tokio::test]
    async fn test_attach_propagates_network_enable_error() {
        let (url, mut conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();
        let (mut reader, mut writer) = conns.recv().await.unwrap();

        let cdp_clone = cdp.clone();
        let handle = tokio::spawn(async move { cdp_clone.attach("TARGET_NE", None).await });

        // 1. Target.attachToTarget succeeds
        let msg = read_json(&mut reader).await;
        assert_eq!(msg["method"], "Target.attachToTarget");
        send_json(
            &mut writer,
            json!({"id": msg["id"], "result": {"sessionId": "SESS_NE"}}),
        )
        .await;

        // 2. Network.enable returns CDP error
        let msg = read_json(&mut reader).await;
        assert_eq!(msg["method"], "Network.enable");
        send_json(
            &mut writer,
            json!({"id": msg["id"], "error": {"code": -32000, "message": "Network.enable failed"}}),
        )
        .await;

        // 3. attach() rollback sends Target.detachFromTarget — reply to avoid timeout
        let msg = read_json(&mut reader).await;
        assert_eq!(msg["method"], "Target.detachFromTarget");
        send_json(&mut writer, json!({"id": msg["id"], "result": {}})).await;

        let result = handle.await.unwrap();
        assert!(
            result.is_err(),
            "attach should fail when Network.enable errors"
        );

        // Session mapping must be rolled back — no stale half-attached entry.
        assert!(
            cdp.tab_sessions.lock().await.get("TARGET_NE").is_none(),
            "session mapping should be rolled back on Network.enable failure"
        );
    }

    // ── 16. test_attach_auto_attach_failure_is_best_effort ──────────

    /// Target.setAutoAttach failure must NOT cause attach() to fail —
    /// it is an optional capability (OOPIF/iframe support). Basic tab
    /// operation still works without it.
    #[tokio::test]
    async fn test_attach_auto_attach_failure_is_best_effort() {
        let (url, mut conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();
        let (mut reader, mut writer) = conns.recv().await.unwrap();

        let cdp_clone = cdp.clone();
        let handle = tokio::spawn(async move { cdp_clone.attach("TARGET_AA", None).await });

        // 1. Target.attachToTarget succeeds
        let msg = read_json(&mut reader).await;
        send_json(
            &mut writer,
            json!({"id": msg["id"], "result": {"sessionId": "SESS_AA"}}),
        )
        .await;

        // 2. Network.enable succeeds
        let msg = read_json(&mut reader).await;
        assert_eq!(msg["method"], "Network.enable");
        send_json(&mut writer, json!({"id": msg["id"], "result": {}})).await;

        // 3. Target.setAutoAttach FAILS — should not block attach
        let msg = read_json(&mut reader).await;
        assert_eq!(msg["method"], "Target.setAutoAttach");
        send_json(
            &mut writer,
            json!({"id": msg["id"], "error": {"code": -32000, "message": "setAutoAttach failed"}}),
        )
        .await;

        let result = handle.await.unwrap();
        assert!(
            result.is_ok(),
            "attach should succeed even when setAutoAttach fails: {:?}",
            result.err()
        );
    }

    // ── 17. test_attach_stealth_failure_does_not_block ───────────────

    /// Stealth injection errors (Page.enable, addScriptToEvaluateOnNewDocument,
    /// setUserAgentOverride) must NOT cause attach() to fail — they are best-effort.
    #[tokio::test]
    async fn test_attach_stealth_failure_does_not_block() {
        let (url, mut conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();
        let (mut reader, mut writer) = conns.recv().await.unwrap();

        let cdp_clone = cdp.clone();
        let handle = tokio::spawn(async move {
            cdp_clone
                .attach("TARGET_ST", Some("Mozilla/5.0 FakeUA"))
                .await
        });

        // 1. Target.attachToTarget succeeds
        let msg = read_json(&mut reader).await;
        send_json(
            &mut writer,
            json!({"id": msg["id"], "result": {"sessionId": "SESS_ST"}}),
        )
        .await;

        // 2. Network.enable succeeds
        let msg = read_json(&mut reader).await;
        send_json(&mut writer, json!({"id": msg["id"], "result": {}})).await;

        // 3. Target.setAutoAttach succeeds
        let msg = read_json(&mut reader).await;
        send_json(&mut writer, json!({"id": msg["id"], "result": {}})).await;

        // 4. Page.enable FAILS
        let msg = read_json(&mut reader).await;
        assert_eq!(msg["method"], "Page.enable");
        send_json(
            &mut writer,
            json!({"id": msg["id"], "error": {"code": -32000, "message": "Page.enable failed"}}),
        )
        .await;

        // 5. addScriptToEvaluateOnNewDocument FAILS
        let msg = read_json(&mut reader).await;
        assert_eq!(msg["method"], "Page.addScriptToEvaluateOnNewDocument");
        send_json(
            &mut writer,
            json!({"id": msg["id"], "error": {"code": -32000, "message": "script failed"}}),
        )
        .await;

        // 6. setUserAgentOverride FAILS
        let msg = read_json(&mut reader).await;
        assert_eq!(msg["method"], "Emulation.setUserAgentOverride");
        send_json(
            &mut writer,
            json!({"id": msg["id"], "error": {"code": -32000, "message": "ua failed"}}),
        )
        .await;

        // attach() should still succeed despite all stealth errors
        let result = handle.await.unwrap();
        assert!(
            result.is_ok(),
            "attach should succeed even when stealth fails: {:?}",
            result.err()
        );
        assert_eq!(result.unwrap(), "SESS_ST");
    }

    // ── 18. test_detach_cleans_event_subs ────────────────────────────

    /// detach() must clean up all event subscriptions for the detached session.
    #[tokio::test]
    async fn test_detach_cleans_event_subs() {
        let (url, mut conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();
        let (mut reader, mut writer) = conns.recv().await.unwrap();

        // Pre-populate session mapping (skip full attach handshake)
        cdp.tab_sessions
            .lock()
            .await
            .insert("TARGET_DC".to_string(), "SESS_DC".to_string());

        // Subscribe to some events
        let _rx1 = cdp
            .subscribe_events("SESS_DC", "Network.requestWillBeSent")
            .await;
        let _rx2 = cdp.subscribe_events("SESS_DC", "Page.loadEventFired").await;
        // Also subscribe for a different session (should NOT be cleaned)
        let _rx3 = cdp
            .subscribe_events("SESS_OTHER", "Network.requestWillBeSent")
            .await;

        // Verify subscriptions exist
        assert_eq!(cdp.event_subs.lock().await.len(), 3);

        // Detach
        let cdp_clone = cdp.clone();
        let handle = tokio::spawn(async move { cdp_clone.detach("TARGET_DC").await });

        let msg = read_json(&mut reader).await;
        assert_eq!(msg["method"], "Target.detachFromTarget");
        send_json(&mut writer, json!({"id": msg["id"], "result": {}})).await;
        handle.await.unwrap().unwrap();

        // SESS_DC subscriptions should be removed, SESS_OTHER should remain
        let subs = cdp.event_subs.lock().await;
        assert_eq!(
            subs.len(),
            1,
            "only SESS_OTHER subscription should remain, got: {:?}",
            subs.keys().collect::<Vec<_>>()
        );
        assert!(subs.contains_key("SESS_OTHER:Network.requestWillBeSent"));
    }

    // ── 19. test_network_counter_ignores_cache ──────────────────────

    /// Network.requestServedFromCache must NOT decrement the pending counter
    /// because the corresponding requestWillBeSent + loadingFinished pair
    /// already handles the request lifecycle.
    #[tokio::test]
    async fn test_network_counter_ignores_cache() {
        let (url, mut conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();
        let (_, mut writer) = conns.recv().await.unwrap();

        cdp.tab_sessions
            .lock()
            .await
            .insert("T_CACHE".to_string(), "S_CACHE".to_string());

        // requestWillBeSent → counter = 1
        send_json(
            &mut writer,
            json!({
                "method": "Network.requestWillBeSent",
                "sessionId": "S_CACHE",
                "params": { "requestId": "r1", "frameId": "T_CACHE" }
            }),
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(cdp.network_pending("S_CACHE").await, 1);

        // requestServedFromCache should NOT decrement
        send_json(
            &mut writer,
            json!({
                "method": "Network.requestServedFromCache",
                "sessionId": "S_CACHE",
                "params": { "requestId": "r1", "frameId": "T_CACHE" }
            }),
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(
            cdp.network_pending("S_CACHE").await,
            1,
            "requestServedFromCache must not decrement counter"
        );

        // loadingFinished brings it back to 0
        send_json(
            &mut writer,
            json!({
                "method": "Network.loadingFinished",
                "sessionId": "S_CACHE",
                "params": { "requestId": "r1", "frameId": "T_CACHE" }
            }),
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(cdp.network_pending("S_CACHE").await, 0);
    }

    fn sample_request(
        request_id: &str,
        url: &str,
        method: &str,
        resource_type: &str,
        status: Option<u16>,
    ) -> TrackedRequest {
        let mut request_headers = HashMap::new();
        request_headers.insert("accept".to_string(), "application/json".to_string());

        let mut response_headers = HashMap::new();
        response_headers.insert("content-type".to_string(), "application/json".to_string());
        response_headers.insert("x-ab-fixture".to_string(), "api-data".to_string());

        TrackedRequest {
            request_id: request_id.to_string(),
            url: url.to_string(),
            method: method.to_string(),
            resource_type: resource_type.to_string(),
            timestamp_ms: 1_712_793_600_000,
            status,
            mime_type: Some("application/json".to_string()),
            request_headers,
            post_data: None,
            response_headers,
            response_body: Some(r#"{"ok":true}"#.to_string()),
        }
    }

    #[test]
    fn test_tracked_request_storage_updates_status_headers_and_mime() {
        let mut requests = VecDeque::new();

        record_request_will_be_sent(
            &mut requests,
            &json!({
                "requestId": "req-1",
                "type": "Fetch",
                "timestamp": 1712793600.0,
                "request": {
                    "url": "http://127.0.0.1/api/data?source=fetch",
                    "method": "GET",
                    "headers": { "accept": "application/json" }
                }
            }),
        );
        record_response_received(
            &mut requests,
            &json!({
                "requestId": "req-1",
                "type": "Fetch",
                "response": {
                    "url": "http://127.0.0.1/api/data?source=fetch",
                    "status": 200,
                    "mimeType": "application/json",
                    "headers": {
                        "content-type": "application/json",
                        "x-ab-fixture": "api-data"
                    }
                }
            }),
        );

        let req = tracked_request_detail(&requests, "req-1").expect("request stored");
        assert_eq!(req.url, "http://127.0.0.1/api/data?source=fetch");
        assert_eq!(req.method, "GET");
        assert_eq!(req.resource_type, "Fetch");
        assert_eq!(req.status, Some(200));
        assert_eq!(req.mime_type.as_deref(), Some("application/json"));
        assert_eq!(
            req.response_headers.get("x-ab-fixture").map(String::as_str),
            Some("api-data")
        );
    }

    #[test]
    fn test_tracked_request_fifo_eviction_drops_oldest_after_500() {
        let mut requests = VecDeque::new();

        for idx in 0..(MAX_TRACKED_REQUESTS + 1) {
            record_request_will_be_sent(
                &mut requests,
                &json!({
                    "requestId": format!("req-{idx}"),
                    "type": "XHR",
                    "timestamp": 1712793600.0 + idx as f64,
                    "request": {
                        "url": format!("http://127.0.0.1/api/data?i={idx}"),
                        "method": "GET",
                        "headers": {}
                    }
                }),
            );
        }

        assert_eq!(requests.len(), MAX_TRACKED_REQUESTS);
        assert!(tracked_request_detail(&requests, "req-0").is_none());
        assert!(tracked_request_detail(&requests, "req-500").is_some());
    }

    #[test]
    fn test_filter_tracked_requests_by_url_substring() {
        let requests = VecDeque::from([
            sample_request(
                "req-1",
                "http://127.0.0.1/page-a",
                "GET",
                "Document",
                Some(200),
            ),
            sample_request(
                "req-2",
                "http://127.0.0.1/api/data?source=fetch",
                "GET",
                "Fetch",
                Some(200),
            ),
        ]);

        let filtered = filter_tracked_requests(
            &requests,
            &NetworkRequestsFilter {
                url_substring: Some("/api/data".to_string()),
                ..NetworkRequestsFilter::default()
            },
        );

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].request_id, "req-2");
    }

    #[test]
    fn test_filter_tracked_requests_by_resource_type_case_insensitive_csv() {
        let requests = VecDeque::from([
            sample_request(
                "req-1",
                "http://127.0.0.1/page-a",
                "GET",
                "Document",
                Some(200),
            ),
            sample_request(
                "req-2",
                "http://127.0.0.1/api/data?source=fetch",
                "GET",
                "Fetch",
                Some(200),
            ),
            sample_request(
                "req-3",
                "http://127.0.0.1/api/data?source=xhr",
                "POST",
                "XHR",
                Some(201),
            ),
        ]);

        let filtered = filter_tracked_requests(
            &requests,
            &NetworkRequestsFilter {
                resource_types: Some("xhr,fetch".to_string()),
                ..NetworkRequestsFilter::default()
            },
        );

        assert_eq!(filtered.len(), 2);
        assert!(
            filtered
                .iter()
                .all(|req| { req.resource_type == "Fetch" || req.resource_type == "XHR" })
        );
    }

    #[test]
    fn test_filter_tracked_requests_by_method_case_insensitive() {
        let requests = VecDeque::from([
            sample_request(
                "req-1",
                "http://127.0.0.1/page-a",
                "GET",
                "Document",
                Some(200),
            ),
            sample_request(
                "req-2",
                "http://127.0.0.1/api/data?source=xhr",
                "POST",
                "XHR",
                Some(201),
            ),
        ]);

        let filtered = filter_tracked_requests(
            &requests,
            &NetworkRequestsFilter {
                method: Some("post".to_string()),
                ..NetworkRequestsFilter::default()
            },
        );

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].request_id, "req-2");
    }

    #[test]
    fn test_filter_tracked_requests_by_status_exact_class_and_range() {
        let requests = VecDeque::from([
            sample_request(
                "req-1",
                "http://127.0.0.1/page-a",
                "GET",
                "Document",
                Some(200),
            ),
            sample_request(
                "req-2",
                "http://127.0.0.1/api/data?source=create",
                "POST",
                "XHR",
                Some(201),
            ),
            sample_request(
                "req-3",
                "http://127.0.0.1/api/data?source=error",
                "GET",
                "Fetch",
                Some(404),
            ),
        ]);

        let exact = filter_tracked_requests(
            &requests,
            &NetworkRequestsFilter {
                status: Some("200".to_string()),
                ..NetworkRequestsFilter::default()
            },
        );
        let class = filter_tracked_requests(
            &requests,
            &NetworkRequestsFilter {
                status: Some("2xx".to_string()),
                ..NetworkRequestsFilter::default()
            },
        );
        let range = filter_tracked_requests(
            &requests,
            &NetworkRequestsFilter {
                status: Some("400-499".to_string()),
                ..NetworkRequestsFilter::default()
            },
        );

        assert_eq!(
            exact
                .iter()
                .map(|r| r.request_id.as_str())
                .collect::<Vec<_>>(),
            vec!["req-1"]
        );
        assert_eq!(
            class
                .iter()
                .map(|r| r.request_id.as_str())
                .collect::<Vec<_>>(),
            vec!["req-1", "req-2"]
        );
        assert_eq!(
            range
                .iter()
                .map(|r| r.request_id.as_str())
                .collect::<Vec<_>>(),
            vec!["req-3"]
        );
    }

    #[test]
    fn test_filter_tracked_requests_with_combined_filters() {
        let requests = VecDeque::from([
            sample_request(
                "req-1",
                "http://127.0.0.1/api/data?source=fetch",
                "GET",
                "Fetch",
                Some(200),
            ),
            sample_request(
                "req-2",
                "http://127.0.0.1/api/data?source=xhr",
                "POST",
                "XHR",
                Some(201),
            ),
            sample_request(
                "req-3",
                "http://127.0.0.1/asset.js",
                "GET",
                "Script",
                Some(200),
            ),
        ]);

        let filtered = filter_tracked_requests(
            &requests,
            &NetworkRequestsFilter {
                url_substring: Some("/api/data".to_string()),
                resource_types: Some("xhr".to_string()),
                method: Some("POST".to_string()),
                status: Some("2xx".to_string()),
            },
        );

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].request_id, "req-2");
    }

    #[test]
    fn test_clear_tracked_requests_resets_list() {
        let mut requests = VecDeque::from([
            sample_request(
                "req-1",
                "http://127.0.0.1/page-a",
                "GET",
                "Document",
                Some(200),
            ),
            sample_request(
                "req-2",
                "http://127.0.0.1/api/data?source=fetch",
                "GET",
                "Fetch",
                Some(200),
            ),
        ]);

        let cleared = clear_tracked_requests(&mut requests);
        assert_eq!(cleared, 2);
        assert!(requests.is_empty());
    }

    #[test]
    fn test_tracked_request_detail_returns_headers_and_response_body() {
        let requests = VecDeque::from([sample_request(
            "req-9",
            "http://127.0.0.1/api/data?source=fetch",
            "GET",
            "Fetch",
            Some(200),
        )]);

        let req = tracked_request_detail(&requests, "req-9").expect("detail entry");
        assert_eq!(
            req.request_headers.get("accept").map(String::as_str),
            Some("application/json")
        );
        assert_eq!(
            req.response_headers.get("content-type").map(String::as_str),
            Some("application/json")
        );
        assert_eq!(req.response_body.as_deref(), Some(r#"{"ok":true}"#));
    }
}

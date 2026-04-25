// Actionbook Browser Bridge - Background Service Worker
// Connects to either the local CLI bridge (`local` mode) or the Cloudflare
// edge-server (`cloud` mode) via WebSocket and executes browser commands.
//
// Mode is read from chrome.storage.local.mode; default "cloud" — local mode
// is opt-in for advanced users running their own CLI bridge.

const LOCAL_BRIDGE_URL = "ws://127.0.0.1:19222";
const DEFAULT_CLOUD_ENDPOINT = "wss://edge.actionbook.dev/extension/ws";
const RECONNECT_BASE_MS = 1000;
const RECONNECT_MAX_MS = 30000;
const MAX_RETRIES = 8;
const BRIDGE_PROBE_TIMEOUT_MS = 750;

const HANDSHAKE_TIMEOUT_MS = 2000;
const L3_CONFIRM_TIMEOUT_MS = 30000;

// Protocol version reported in the hello frame. Bumped to 0.5.0 for cloud mode.
const PROTOCOL_VERSION = "0.5.0";

// --- Tab Group Config ---
// All tabs opened by Actionbook (via Extension.createTab or the reuse-empty-tab
// path) are moved into a per-window Chrome tab group titled "Actionbook" so
// users can tell agent-driven tabs apart at a glance and collapse/close them
// in bulk. The group is looked up by title in the tab's own window — we do
// NOT persist groupId, since it's unstable across sessions and windows.
const ACTIONBOOK_GROUP_TITLE = "⋈ Actionbook";
const ACTIONBOOK_GROUP_COLOR = "grey";
// User-facing toggle (chrome.storage.local key: "groupTabs"). Default on.
let groupingEnabled = true;

// --- CDP Method Allowlist ---

const CDP_ALLOWLIST = {
  // L1 - Read only (auto-approved)
  'Page.captureScreenshot': 'L1',
  'Page.getFrameTree': 'L1',
  'Page.getLayoutMetrics': 'L1',
  'Page.getNavigationHistory': 'L1',
  'DOM.getDocument': 'L1',
  'DOM.querySelector': 'L1',
  'DOM.querySelectorAll': 'L1',
  'DOM.getOuterHTML': 'L1',
  'DOM.enable': 'L1',
  'DOM.describeNode': 'L1',
  'DOM.getBoxModel': 'L1',
  'DOM.getFrameOwner': 'L1',
  'DOM.getNodeForLocation': 'L1',
  'DOM.requestNode': 'L1',
  'DOM.resolveNode': 'L1',
  'Accessibility.enable': 'L1',
  'Accessibility.getFullAXTree': 'L1',
  'Accessibility.getPartialAXTree': 'L1',
  'Accessibility.queryAXTree': 'L1',
  'Network.getCookies': 'L1',
  'Network.getAllCookies': 'L1',
  'Network.getResponseBody': 'L1',
  // Network.enable is required before Chrome emits Network.requestWillBeSent /
  // responseReceived / loadingFinished events. HAR recording (`browser network
  // har start`) depends on those events — without enabling the Network domain
  // the recorder sees zero traffic and har stop returns count=0.
  'Network.enable': 'L1',
  'Network.disable': 'L1',

  // L2 - Page modification (auto-approved with logging)
  'Runtime.evaluate': 'L2',
  'Runtime.callFunctionOn': 'L2',
  'Page.enable': 'L2',
  'Page.navigate': 'L2',
  'Page.navigateToHistoryEntry': 'L2',
  'Page.reload': 'L2',
  'Target.activateTarget': 'L2',
  'Target.attachToTarget': 'L2',
  'Target.detachFromTarget': 'L2',
  'Target.getTargets': 'L2',
  'Target.setAutoAttach': 'L2',
  'Input.dispatchMouseEvent': 'L2',
  'Input.dispatchKeyEvent': 'L2',
  'DOM.focus': 'L2',
  'DOM.setFileInputFiles': 'L2',
  'Emulation.setDeviceMetricsOverride': 'L2',
  'Emulation.clearDeviceMetricsOverride': 'L2',
  'Page.printToPDF': 'L2',

  // L3 - High risk (requires confirmation)
  'Network.setCookie': 'L3',
  'Network.deleteCookies': 'L3',
  'Network.clearBrowserCookies': 'L3',
  'Page.setDownloadBehavior': 'L3',
  'Storage.clearDataForOrigin': 'L3',
};

const SENSITIVE_DOMAIN_PATTERNS = [
  /\.bank\./i, /\.banking\./i, /banking\./i,
  /pay\./i, /payment\./i, /\.payment\./i,
  /\.gov$/i, /\.gov\./i,
  /\.healthcare\./i, /\.health\./i,
  /checkout/i, /billing/i,
];

let ws = null;

// Cleanly tear down the current WebSocket before reconnecting. Without this,
// `ws.close()` fires the old socket's onclose asynchronously, which unconditionally
// mutates module-level state (`ws = null`, `connectionState`, reconnect polling).
// If a new `connect()` has already created a replacement socket by then, the
// stale onclose clobbers the new `ws` reference and drives the state machine
// backwards — especially nasty during mode switches / auth updates / sign-out,
// all of which close-then-reconnect back-to-back.
//
// Solution: detach all handlers first so the old socket's death is silent, then
// close and null out `ws`. Callers free to call `connect()` immediately after.
function detachAndCloseWs(reason) {
  if (!ws) return;
  const oldWs = ws;
  ws = null;
  try {
    oldWs.onopen = null;
    oldWs.onmessage = null;
    oldWs.onerror = null;
    oldWs.onclose = null;
  } catch (_) {
    /* ignore */
  }
  try {
    oldWs.close(1000, reason || "replaced");
  } catch (_) {
    /* ignore */
  }
}

// Set<number> of Chrome tab IDs that the extension currently has
// chrome.debugger attached to. Multiple tabs can be attached concurrently;
// every CDP command from the CLI must carry its target `tabId` field.
// The legacy single-attach variable and `Extension.attachActiveTab` have
// been removed — the CLI always specifies tabId.
const attachedTabs = new Set();
let connectionState = "idle"; // idle | pairing_required | connecting | connected | disconnected | failed
let reconnectDelay = RECONNECT_BASE_MS;
let reconnectTimer = null;
let retryCount = 0;
let lastLoggedState = null;
let handshakeTimer = null;
let handshakeCompleted = false;
let wasReplaced = false; // true when bridge notified us another extension instance took over

// L3 confirmation state: pending command waiting for user approval
let pendingL3 = null; // { id, method, params, domain, nonce, resolve }
let l3NonceCounter = 0;

// --- Debug Logging ---
// Set to true for development diagnostics; false silences all console output in production.
const DEBUG_ENABLED = false;

function debugLog(...args) {
  if (DEBUG_ENABLED) console.log(...args);
}

function debugError(...args) {
  if (DEBUG_ENABLED) console.error(...args);
}

// --- Token Format Validation ---
// NOTE: Token validation removed in v0.8.0 - bridge now uses localhost trust model
// Legacy compatibility: accept any truthy value as valid

// --- Offscreen Document for SW Keep-alive ---

async function ensureOffscreenDocument() {
  const existingContexts = await chrome.runtime.getContexts({
    contextTypes: ["OFFSCREEN_DOCUMENT"],
  });
  if (existingContexts.length > 0) return;

  try {
    await chrome.offscreen.createDocument({
      url: "offscreen.html",
      reasons: ["BLOBS"], // MV3 requires a reason; BLOBS is accepted for keep-alive patterns
      justification: "Keep service worker alive for persistent WebSocket connection",
    });
    debugLog("[actionbook] Offscreen document created for keep-alive");
  } catch (err) {
    // Document may already exist from a race condition
    if (!err.message?.includes("Only a single offscreen")) {
      debugError("[actionbook] Failed to create offscreen document:", err);
    }
  }
}

// --- WebSocket Connection Management ---

function logStateTransition(newState, detail) {
  if (newState !== lastLoggedState) {
    const msg = detail
      ? `[actionbook] State: ${lastLoggedState} -> ${newState} (${detail})`
      : `[actionbook] State: ${lastLoggedState} -> ${newState}`;
    debugLog(msg);
    lastLoggedState = newState;
  }
}

// Resolve the current connection mode + URL from chrome.storage.local.
// Returns { mode, url, token, deviceId } — token/deviceId are undefined in local mode.
// Default mode is "cloud"; local is opt-in for users running their own CLI bridge.
async function getConnectionConfig() {
  const { mode, cloudEndpoint, cloudToken, deviceId } = await chrome.storage.local.get([
    "mode",
    "cloudEndpoint",
    "cloudToken",
    "deviceId",
  ]);
  if (mode === "local") {
    return { mode: "local", url: LOCAL_BRIDGE_URL, token: null, deviceId: null };
  }
  return {
    mode: "cloud",
    url: cloudEndpoint || DEFAULT_CLOUD_ENDPOINT,
    token: cloudToken || null,
    deviceId: deviceId || null,
  };
}

async function getEffectiveBridgeUrl() {
  const cfg = await getConnectionConfig();
  return cfg.url;
}

// Try to refresh the cloud access token using the stored refresh_token.
// Returns the new access_token (and persists it), or null if refresh failed
// (missing refresh_token, network error, Clerk rejection — caller should then
// force the user through sign-in again).
//
// Called opportunistically before a connect when the stored token is close to
// or past expiry, and reactively if a connect attempt fails with auth-related
// errors.
async function refreshCloudTokenIfNeeded({ force = false } = {}) {
  const { cloudToken, cloudRefreshToken, cloudTokenExpiresAt } = await chrome.storage.local.get([
    "cloudToken",
    "cloudRefreshToken",
    "cloudTokenExpiresAt",
  ]);

  // Refresh 60s before expiry to avoid mid-connect expiry.
  const REFRESH_SKEW_MS = 60_000;
  const needsRefresh =
    force ||
    !cloudToken ||
    (typeof cloudTokenExpiresAt === "number" && Date.now() + REFRESH_SKEW_MS >= cloudTokenExpiresAt);

  if (!needsRefresh) {
    // Token still fresh — return it so callers (pre-flight connect) can short-circuit.
    // A missing refresh_token is fine in this branch: we're not refreshing anyway.
    return cloudToken;
  }

  // From here on we actually need a new token. Without a refresh_token we can't
  // get one; return null so callers (especially the invalid_token path) can
  // transition to sign-in-required instead of retrying with the same rejected
  // access token indefinitely.
  if (!cloudRefreshToken) return null;

  // cloud-config.js constants are not loaded in the service worker (it's a plain
  // worker, not a module + no importScripts in MV3). Hardcode here; if you
  // change the Clerk tenant, update both places.
  const CLERK_TOKEN_URL = "https://clerk.actionbook.dev/oauth/token";
  const CLERK_CLIENT_ID = "HP91Xj6adCm3TjPr";

  try {
    const res = await fetch(CLERK_TOKEN_URL, {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({
        grant_type: "refresh_token",
        refresh_token: cloudRefreshToken,
        client_id: CLERK_CLIENT_ID,
      }),
    });
    if (!res.ok) {
      debugLog("[actionbook] refresh failed:", res.status, await res.text());
      return null;
    }
    const tokens = await res.json();
    if (!tokens.access_token) return null;

    const update = { cloudToken: tokens.access_token };
    if (typeof tokens.refresh_token === "string") {
      update.cloudRefreshToken = tokens.refresh_token;
    }
    if (typeof tokens.expires_in === "number") {
      update.cloudTokenExpiresAt = Date.now() + tokens.expires_in * 1000;
    }
    await chrome.storage.local.set(update);
    return tokens.access_token;
  } catch (err) {
    debugLog("[actionbook] refresh error:", err?.message || err);
    return null;
  }
}

// Lazily generate and persist a per-install deviceId (only used in cloud mode).
async function ensureDeviceId() {
  const { deviceId } = await chrome.storage.local.get("deviceId");
  if (deviceId) return deviceId;
  const fresh = (self.crypto && typeof self.crypto.randomUUID === "function")
    ? `d_${self.crypto.randomUUID()}`
    : `d_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 10)}`;
  await chrome.storage.local.set({ deviceId: fresh });
  return fresh;
}

function getBridgeHealthUrl(bridgeUrl) {
  if (bridgeUrl.startsWith("ws://")) {
    return `http://${bridgeUrl.slice("ws://".length)}/healthz`;
  }
  if (bridgeUrl.startsWith("wss://")) {
    return `https://${bridgeUrl.slice("wss://".length)}/healthz`;
  }
  return `${bridgeUrl}/healthz`;
}

async function canReachBridge(bridgeUrl) {
  const controller = new AbortController();
  const timeoutId = setTimeout(() => controller.abort(), BRIDGE_PROBE_TIMEOUT_MS);

  try {
    const response = await fetch(getBridgeHealthUrl(bridgeUrl), {
      method: "HEAD",
      cache: "no-store",
      signal: controller.signal,
    });
    return response.ok;
  } catch (err) {
    debugLog("[actionbook] Bridge probe failed:", err?.message || err);
    return false;
  } finally {
    clearTimeout(timeoutId);
  }
}

async function connect() {
  if (ws && ws.readyState === WebSocket.OPEN) return;
  if (connectionState === "connecting") return;

  connectionState = "connecting";
  logStateTransition("connecting");
  broadcastState();

  let cfg = await getConnectionConfig();

  // Cloud mode: rotate expiring tokens before we even try the WS handshake.
  // `refreshCloudTokenIfNeeded` is a no-op when the token is fresh. If it
  // returns null we fall through to the "no token" branch and wait for sign-in.
  if (cfg.mode === "cloud") {
    const refreshed = await refreshCloudTokenIfNeeded();
    if (refreshed && refreshed !== cfg.token) {
      cfg = await getConnectionConfig();
    }
  }

  // Cloud mode requires a token. If missing, wait for popup-driven login
  // instead of retrying pointlessly. State "pairing_required" already signals
  // this in the popup UI.
  if (cfg.mode === "cloud" && !cfg.token) {
    connectionState = "pairing_required";
    logStateTransition("pairing_required", "cloud mode without token");
    broadcastState();
    return;
  }

  try {
    // Skip healthz probe in cloud mode: the endpoint is always up (Cloudflare
    // hosted) and HEAD /healthz may not be implemented. Go straight to WS.
    if (cfg.mode === "local" && !(await canReachBridge(cfg.url))) {
      connectionState = "disconnected";
      logStateTransition("disconnected", "bridge not listening");
      broadcastState();
      scheduleReconnect();
      return;
    }
    ws = cfg.mode === "cloud"
      ? new WebSocket(cfg.url, ["bearer", cfg.token])
      : new WebSocket(cfg.url);
  } catch (err) {
    connectionState = "disconnected";
    logStateTransition("disconnected", "WebSocket constructor error");
    broadcastState();
    scheduleReconnect();
    return;
  }

  handshakeCompleted = false;
  let wsOpened = false;

  ws.onopen = async () => {
    wsOpened = true;
    // Local mode: tokenless, origin-validated. Cloud mode: token already went
    // up via Sec-WebSocket-Protocol at upgrade; hello is used to report the
    // per-install deviceId for device registry.
    const hello = {
      type: "hello",
      role: "extension",
      version: PROTOCOL_VERSION,
    };
    if (cfg.mode === "cloud") {
      hello.deviceId = await ensureDeviceId();
    }
    wsSend(hello);

    // Start handshake timeout - if no hello_ack within this window, treat as auth failure
    handshakeTimer = setTimeout(() => {
      handshakeTimer = null;
      if (!handshakeCompleted) {
        // Timeout without ack = likely bad token or old server version
        connectionState = "pairing_required";
        logStateTransition("pairing_required", "handshake timeout (no hello_ack)");
        broadcastState();
        if (ws) {
          ws.close();
          ws = null;
        }
      }
    }, HANDSHAKE_TIMEOUT_MS);
  };

  ws.onmessage = async (event) => {
    let msg;
    try {
      msg = JSON.parse(event.data);
    } catch (err) {
      return;
    }

    // Handle hello_ack from server (explicit auth confirmation)
    if (!handshakeCompleted && msg.type === "hello_ack") {
      handshakeCompleted = true;
      if (handshakeTimer) {
        clearTimeout(handshakeTimer);
        handshakeTimer = null;
      }
      connectionState = "connected";
      retryCount = 0;
      reconnectDelay = RECONNECT_BASE_MS;
      stopBridgePolling();
      logStateTransition("connected");
      broadcastState();
      return;
    }

    // Handle token_expired from server (token rotated due to inactivity)
    if (msg.type === "token_expired") {
      connectionState = "pairing_required";
      logStateTransition("pairing_required", "token expired by server");
      broadcastState();
      startBridgePolling();
      if (ws) { ws.close(); ws = null; }
      return;
    }

    // Handle replaced notification: another extension instance connected to the bridge.
    // Stop reconnecting to avoid an infinite connect/disconnect loop.
    if (msg.type === "replaced") {
      debugLog("[actionbook] Replaced by another extension instance");
      wasReplaced = true;
      connectionState = "failed";
      logStateTransition("failed", "replaced by another extension instance");
      stopBridgePolling();
      if (reconnectTimer) {
        clearTimeout(reconnectTimer);
        reconnectTimer = null;
      }
      broadcastState();
      // Server will close the WebSocket after this message
      return;
    }

    // Handle hello_error from server (version mismatch, invalid token, etc.)
    if (msg.type === "hello_error") {
      debugLog("[actionbook] Server rejected handshake:", msg.message);
      handshakeCompleted = false;
      if (handshakeTimer) {
        clearTimeout(handshakeTimer);
        handshakeTimer = null;
      }
      // Detach handlers before close — the invalid_token branch below will
      // await a token refresh and then call connect(), and we don't want the
      // old socket's onclose to race with the new one mid-refresh.
      detachAndCloseWs("hello_error");

      if (msg.error === "invalid_token") {
        // Cloud mode: try a one-shot refresh before giving up. If refresh
        // succeeds, kick off a reconnect with the fresh token. If it fails
        // we drop into pairing_required so popup prompts the user to sign in
        // again.
        const fresh = await refreshCloudTokenIfNeeded({ force: true });
        if (fresh) {
          connectionState = "disconnected";
          logStateTransition("disconnected", "token refreshed, reconnecting");
          broadcastState();
          retryCount = 0;
          reconnectDelay = RECONNECT_BASE_MS;
          connect();
        } else {
          // Terminal auth failure: clear the cached token bundle so polling
          // can't keep retrying with credentials the server already rejected.
          await chrome.storage.local.remove([
            "cloudToken",
            "cloudRefreshToken",
            "cloudTokenExpiresAt",
          ]);
          connectionState = "pairing_required";
          logStateTransition("pairing_required", "invalid token, refresh failed");
          broadcastState();
          startBridgePolling();
        }
      } else {
        connectionState = "failed";
        logStateTransition("failed", msg.message || "handshake rejected by server");
        broadcastState();
        startBridgePolling();
      }
      return;
    }

    // Normal command message - must be authenticated first
    if (!handshakeCompleted) return;

    const response = await handleCommand(msg);
    wsSend(response);
  };

  ws.onclose = () => {
    ws = null;

    if (handshakeTimer) {
      clearTimeout(handshakeTimer);
      handshakeTimer = null;
    }

    // If we were replaced by another extension instance, stay stopped.
    if (wasReplaced) {
      connectionState = "failed";
      logStateTransition("failed", "replaced by another extension instance");
      broadcastState();
      return;
    }

    if (!handshakeCompleted) {
      if (!wsOpened) {
        // Connection never opened - network error (server down, etc.)
        connectionState = "disconnected";
        logStateTransition("disconnected", "connection refused (server not running?)");
        broadcastState();
        startBridgePolling();
        scheduleReconnect();
      } else {
        connectionState = "pairing_required";
        logStateTransition("pairing_required", "handshake failed");
        broadcastState();
        startBridgePolling();
      }
      return;
    }

    // Was connected, now disconnected - bridge may have stopped.
    connectionState = "disconnected";
    logStateTransition("disconnected", "bridge connection lost");
    broadcastState();
    startBridgePolling();
  };

  ws.onerror = () => {
    // onclose will fire after onerror, triggering reconnect
  };
}

function wsSend(data) {
  if (ws && ws.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify(data));
  }
}

function scheduleReconnect() {
  if (wasReplaced) return;

  if (reconnectTimer) {
    clearTimeout(reconnectTimer);
  }

  retryCount++;

  if (retryCount > MAX_RETRIES) {
    connectionState = "failed";
    logStateTransition("failed", `retries exhausted (${MAX_RETRIES})`);
    broadcastState();
    return;
  }

  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    connect();
  }, reconnectDelay);

  // Exponential backoff: double delay, cap at max
  reconnectDelay = Math.min(reconnectDelay * 2, RECONNECT_MAX_MS);
  broadcastState();
}

// --- CDP Event Forwarding ---

chrome.debugger.onEvent.addListener((source, method, params) => {
  if (typeof source.tabId === "number" && attachedTabs.has(source.tabId)) {
    // Forward CDP event to bridge (no id field = event, not response).
    // Include `tabId` so CdpSession can route the event to the right tab
    // subscriber in extension-mode multi-tab sessions.
    wsSend({
      method: method,
      params: params || {},
      tabId: source.tabId,
      ...(typeof source.sessionId === "string" ? { sessionId: source.sessionId } : {}),
    });
  }
});

// --- Tab Group Helper ---

// Move a tab into the Actionbook group in its current window, creating the
// group if none exists there. All failures are swallowed: grouping is a UX
// nicety and must never break the CDP command that triggered it. Callers
// should not await any particular outcome — treat this as fire-and-log.
/**
 * Warm the DOM agent for a tab by traversing the full document.
 *
 * `chrome.debugger.attach` does not auto-enable any CDP domains. Just
 * calling `DOM.enable` enables the agent but does NOT pre-populate its
 * internal backendNodeId → node index. `DOM.resolveNode(backendNodeId)`
 * — used by cloud locator for `@eN` ref resolution — then rejects all
 * IDs even when they came from a fresh `Accessibility.getFullAXTree`.
 *
 * `DOM.getDocument({ depth: -1, pierce: false })` is the canonical way
 * to force the DOM agent to traverse the whole document and build its
 * index. After this call, `DOM.resolveNode` works for every backendId
 * present in the document. Idempotent (safe to call multiple times).
 *
 * Failures are logged but not fatal — many cloud commands work without
 * the index (Runtime.evaluate, Page.captureScreenshot, etc.).
 */
async function ensureDomReady(tabId) {
  try {
    await chrome.debugger.sendCommand(
      { tabId },
      "DOM.getDocument",
      { depth: -1, pierce: false },
    );
  } catch (err) {
    debugLog(`[actionbook] DOM.getDocument failed for tab ${tabId}:`, err?.message || err);
  }
}

async function ensureTabInActionbookGroup(tabId) {
  if (!groupingEnabled) return;
  if (typeof tabId !== "number") return;
  // tabGroups API availability guard — if user loaded an older build without
  // the permission, skip silently instead of throwing.
  if (!chrome.tabGroups || !chrome.tabs.group) return;

  try {
    const tab = await chrome.tabs.get(tabId);
    if (!tab || typeof tab.windowId !== "number") return;

    // Look up an existing Actionbook group in THIS window. groupId must be
    // scoped per-window: passing a cross-window groupId to chrome.tabs.group
    // would move the tab to the group's window, which is not what we want.
    const existing = await chrome.tabGroups.query({
      title: ACTIONBOOK_GROUP_TITLE,
      windowId: tab.windowId,
    });

    let groupId;
    if (existing && existing.length > 0) {
      groupId = existing[0].id;
      await chrome.tabs.group({ groupId, tabIds: [tabId] });
    } else {
      groupId = await chrome.tabs.group({
        tabIds: [tabId],
        createProperties: { windowId: tab.windowId },
      });
      await chrome.tabGroups.update(groupId, {
        title: ACTIONBOOK_GROUP_TITLE,
        color: ACTIONBOOK_GROUP_COLOR,
      });
    }
    // Pin the Actionbook group to the leftmost position of the window so
    // agent-driven tabs are always findable in the same spot. We move the
    // underlying tabs (not the group) because chrome.tabGroups.move has
    // known issues moving single-tab groups to index 0 in some Chrome
    // builds. Chrome automatically clamps past any pinned tabs.
    try {
      const groupTabs = await chrome.tabs.query({ groupId });
      const tabIdsToMove = groupTabs
        .sort((a, b) => a.index - b.index)
        .map((t) => t.id);
      if (tabIdsToMove.length > 0) {
        await chrome.tabs.move(tabIdsToMove, { index: 0 });
      }
    } catch (err) {
      console.warn("[actionbook] pin group to leftmost failed:", err?.message || err);
    }
  } catch (err) {
    debugLog("[actionbook] ensureTabInActionbookGroup failed:", err?.message || err);
  }
}

const REUSABLE_EMPTY_TAB_URLS = new Set([
  "about:blank",
  "about:newtab",
  "chrome://newtab/",
  "chrome://new-tab-page/",
  "edge://newtab/",
]);

function tabUrlForReuse(tab) {
  return (tab?.pendingUrl || tab?.url || "").toLowerCase();
}

function isReusableInitialEmptyTab(tab) {
  if (!tab || typeof tab.id !== "number") return false;
  return REUSABLE_EMPTY_TAB_URLS.has(tabUrlForReuse(tab));
}

async function createOrReuseTab(targetUrl) {
  const tabsInCurrentWindow = await chrome.tabs.query({ currentWindow: true });
  if (
    tabsInCurrentWindow.length === 1 &&
    isReusableInitialEmptyTab(tabsInCurrentWindow[0])
  ) {
    const reusableTab = tabsInCurrentWindow[0];
    const tab = await chrome.tabs.update(reusableTab.id, { url: targetUrl, active: true });
    await ensureTabInActionbookGroup(tab.id);
    return { tab, reused: true };
  }

  const tab = await chrome.tabs.create({ url: targetUrl });
  await ensureTabInActionbookGroup(tab.id);
  return { tab, reused: false };
}

// --- Command Handler ---

async function handleCommand(msg) {
  const { id, method, params, tabId, sessionId } = msg;

  if (!method) {
    return { id, error: { code: -32600, message: "Missing method" } };
  }

  try {
    // Extension-specific commands (non-CDP) — tabId lives inside params for
    // these (e.g. Extension.attachTab{tabId:N}), not at the message root.
    if (method.startsWith("Extension.")) {
      return await handleExtensionCommand(id, method, params || {});
    }

    // CDP commands — every command must specify which tab it targets.
    // Protocol 0.3.0: root-level `tabId` is required; no implicit "active".
    if (typeof tabId !== "number") {
      return {
        id,
        error: {
          code: -32602,
          message: `Missing required root-level "tabId" for CDP method ${method} (protocol 0.3.0+)`,
        },
      };
    }
    return await handleCdpCommand(id, method, params || {}, tabId, sessionId);
  } catch (err) {
    return {
      id,
      error: { code: -32000, message: err.message || String(err) },
    };
  }
}

async function handleExtensionCommand(id, method, params) {
  switch (method) {
    case "Extension.ping":
      return { id, result: { status: "pong", timestamp: Date.now() } };

    case "Extension.listTabs": {
      let actionbookGroupIds = new Set();
      if (chrome.tabGroups && chrome.tabGroups.query) {
        try {
          const groups = await chrome.tabGroups.query({
            title: ACTIONBOOK_GROUP_TITLE,
          });
          actionbookGroupIds = new Set(groups.map((g) => g.id));
        } catch (_) {}
      }
      const all = await chrome.tabs.query({});
      const managed = all.filter(
        (t) => typeof t.groupId === "number" && actionbookGroupIds.has(t.groupId)
      );
      const tabList = managed.map((t) => ({
        id: t.id,
        title: t.title,
        url: t.url,
        active: t.active,
        windowId: t.windowId,
      }));
      return { id, result: { tabs: tabList } };
    }

    case "Extension.attachTab": {
      const tabId = params.tabId;
      if (!tabId || typeof tabId !== "number") {
        return { id, error: { code: -32602, message: "Missing or invalid tabId" } };
      }

      // Verify tab exists, capture metadata for the response so callers
      // (e.g. CLI session start) can surface url/title without an extra
      // round-trip.
      let tabInfo;
      try {
        tabInfo = await chrome.tabs.get(tabId);
      } catch (_) {
        return { id, error: { code: -32000, message: `Tab ${tabId} not found` } };
      }

      // Accumulate — never auto-detach other tabs. Protocol 0.3.0 supports
      // concurrent multi-tab attach; detach is explicit via Extension.detachTab.
      if (!attachedTabs.has(tabId)) {
        try {
          await chrome.debugger.attach({ tabId }, "1.3");
          attachedTabs.add(tabId);
        } catch (err) {
          return { id, error: { code: -32000, message: `attach failed: ${err.message}` } };
        }
      }
      // chrome.debugger.attach does not auto-enable any CDP domains. Warm
      // the DOM agent so backendNodeId-based lookups (DOM.resolveNode, etc.)
      // work for refs returned by Accessibility.getFullAXTree.
      await ensureDomReady(tabId);
      await ensureTabInActionbookGroup(tabId);
      broadcastState();
      return {
        id,
        result: {
          attached: true,
          tabId,
          url: tabInfo.url || "",
          title: tabInfo.title || "",
        },
      };
    }

    case "Extension.createTab": {
      const url = params.url || "about:blank";
      const { tab, reused } = await createOrReuseTab(url);

      // Auto-attach the newly-created tab so subsequent CDP commands on it
      // work without a separate attachTab round-trip. Existing attached tabs
      // are untouched (multi-attach).
      try {
        if (!attachedTabs.has(tab.id)) {
          await chrome.debugger.attach({ tabId: tab.id }, "1.3");
          attachedTabs.add(tab.id);
        }
        // Warm the DOM agent — see ensureDomReady() for rationale.
        await ensureDomReady(tab.id);
        broadcastState();
        return { id, result: { tabId: tab.id, title: tab.title || "", url: tab.url || url, attached: true, reused } };
      } catch (err) {
        // Tab created but debugger attach failed — return tab info with attached: false
        return { id, result: { tabId: tab.id, title: tab.title || "", url: tab.url || url, attached: false, attachError: err.message, reused } };
      }
    }

    case "Extension.activateTab": {
      const tabId = params.tabId;
      if (!tabId || typeof tabId !== "number") {
        return { id, error: { code: -32602, message: "Missing or invalid tabId" } };
      }
      try {
        await chrome.tabs.update(tabId, { active: true });
        const tab = await chrome.tabs.get(tabId);

        // Auto-attach (accumulating) so a follow-up CDP command can target it.
        if (!attachedTabs.has(tabId)) {
          await chrome.debugger.attach({ tabId }, "1.3");
          attachedTabs.add(tabId);
        }
        // Warm the DOM agent — see ensureDomReady() for rationale.
        await ensureDomReady(tabId);
        broadcastState();
        return { id, result: { success: true, tabId, title: tab.title, url: tab.url, attached: true } };
      } catch (err) {
        return { id, error: { code: -32000, message: `Failed to activate tab ${tabId}: ${err.message}` } };
      }
    }

    case "Extension.detachTab": {
      // Without explicit tabId, detach ALL attached tabs (used during
      // session close). With tabId, detach just that one. Does NOT close
      // the tab itself — see Extension.closeTabs for that.
      const targets = (typeof params.tabId === "number")
        ? [params.tabId]
        : Array.from(attachedTabs);
      for (const t of targets) {
        if (attachedTabs.has(t)) {
          try { await chrome.debugger.detach({ tabId: t }); } catch (_) {}
          attachedTabs.delete(t);
        }
      }
      broadcastState();
      return { id, result: { detached: true, detachedTabIds: targets } };
    }

    case "Extension.closeTabs": {
      // Detach + chrome.tabs.remove for the given tabIds (or all attached
      // tabs if none specified). Used by `actionbook browser close` so a
      // session that opened tabs cleans them up — symmetric with how
      // local mode kills the chrome process at session close.
      const targets = (Array.isArray(params.tabIds) && params.tabIds.length)
        ? params.tabIds.filter((t) => typeof t === "number")
        : Array.from(attachedTabs);
      // Detach debugger first (chrome.tabs.remove on an attached tab works
      // but the debugger detach event would arrive after, racing with our
      // bookkeeping).
      for (const t of targets) {
        if (attachedTabs.has(t)) {
          try { await chrome.debugger.detach({ tabId: t }); } catch (_) {}
          attachedTabs.delete(t);
        }
      }
      const closed = [];
      const failed = [];
      for (const t of targets) {
        try {
          await chrome.tabs.remove(t);
          closed.push(t);
        } catch (err) {
          failed.push({ tabId: t, error: err && err.message ? err.message : String(err) });
        }
      }
      broadcastState();
      return { id, result: { closed, failed } };
    }

    case "Extension.status": {
      return {
        id,
        result: {
          connected: connectionState === "connected",
          attachedTabIds: Array.from(attachedTabs),
          version: "0.4.0",
        },
      };
    }

    case "Extension.getCookies": {
      // Require a URL to scope cookies — never return cross-domain cookies
      if (!params.url || typeof params.url !== 'string' || !params.url.startsWith('http')) {
        return { id, error: { code: -32602, message: "Missing or invalid 'url' parameter (must be http/https URL)" } };
      }
      try {
        // When a domain filter is provided, use { domain } to get cookies for
        // ALL paths under that domain. { url } only returns cookies whose path
        // matches the URL path, missing cookies scoped to /app, /account, etc.
        const query = (params.domain && typeof params.domain === 'string')
          ? { domain: params.domain }
          : { url: params.url };
        const cookies = await chrome.cookies.getAll(query);
        return { id, result: { cookies } };
      } catch (err) {
        return { id, error: { code: -32000, message: `getCookies failed: ${err.message}` } };
      }
    }

    case "Extension.setCookie": {
      // Validate required parameters
      if (!params.url || typeof params.url !== 'string' || !params.url.startsWith('http')) {
        return { id, error: { code: -32602, message: "Missing or invalid 'url' parameter (must be http/https URL)" } };
      }
      if (!params.name || typeof params.name !== 'string') {
        return { id, error: { code: -32602, message: "Missing or invalid 'name' parameter" } };
      }
      if (typeof params.value !== 'string') {
        return { id, error: { code: -32602, message: "Missing or invalid 'value' parameter" } };
      }
      // L3 gate: require user confirmation on sensitive domains
      const setCookieDomain = extractDomain(params.url);
      if (isSensitiveDomain(setCookieDomain)) {
        const denial = await requestL3Confirmation(id, "Extension.setCookie", setCookieDomain);
        if (denial) return denial;
      }
      const details = {
        url: params.url,
        name: params.name,
        value: params.value,
      };
      if (params.domain) details.domain = params.domain;
      if (params.path) details.path = params.path;
      try {
        const cookie = await chrome.cookies.set(details);
        if (!cookie) {
          return { id, error: { code: -32000, message: "setCookie returned null (invalid parameters or blocked by browser)" } };
        }
        return { id, result: { success: true, cookie } };
      } catch (err) {
        return { id, error: { code: -32000, message: `setCookie failed: ${err.message}` } };
      }
    }

    case "Extension.removeCookie": {
      // Validate required parameters
      if (!params.url || typeof params.url !== 'string' || !params.url.startsWith('http')) {
        return { id, error: { code: -32602, message: "Missing or invalid 'url' parameter (must be http/https URL)" } };
      }
      if (!params.name || typeof params.name !== 'string') {
        return { id, error: { code: -32602, message: "Missing or invalid 'name' parameter" } };
      }
      // L3 gate on sensitive domains
      const removeCookieDomain = extractDomain(params.url);
      if (isSensitiveDomain(removeCookieDomain)) {
        const denial = await requestL3Confirmation(id, "Extension.removeCookie", removeCookieDomain);
        if (denial) return denial;
      }
      try {
        const details = await chrome.cookies.remove({
          url: params.url,
          name: params.name,
        });
        return { id, result: { success: true, details } };
      } catch (err) {
        return { id, error: { code: -32000, message: `removeCookie failed: ${err.message}` } };
      }
    }

    case "Extension.clearCookies": {
      // Require a URL to scope — never allow cross-domain cookie wipe
      if (!params.url || typeof params.url !== 'string' || !params.url.startsWith('http')) {
        return { id, error: { code: -32602, message: "Missing or invalid 'url' parameter (must be http/https URL). Cannot clear cookies without a URL scope." } };
      }
      // L3 gate on sensitive domains
      const clearCookieDomain = (params.domain && typeof params.domain === 'string')
        ? params.domain.replace(/^\./, "")
        : extractDomain(params.url);
      if (isSensitiveDomain(clearCookieDomain)) {
        const denial = await requestL3Confirmation(id, "Extension.clearCookies", clearCookieDomain);
        if (denial) return denial;
      }
      try {
        // When a domain filter is provided, use { domain } to find cookies for
        // ALL paths, not just the root path that { url } would match.
        const query = (params.domain && typeof params.domain === 'string')
          ? { domain: params.domain }
          : { url: params.url };
        const cookies = await chrome.cookies.getAll(query);
        const removals = cookies.map((c) => {
          const proto = c.secure ? "https" : "http";
          const cookieUrl = `${proto}://${c.domain.replace(/^\./, "")}${c.path}`;
          return chrome.cookies.remove({ url: cookieUrl, name: c.name });
        });
        await Promise.allSettled(removals);
        return { id, result: { success: true, cleared: cookies.length } };
      } catch (err) {
        return { id, error: { code: -32000, message: `clearCookies failed: ${err.message}` } };
      }
    }

    default:
      return {
        id,
        error: { code: -32601, message: `Unknown extension method: ${method}` },
      };
  }
}

async function getTabDomain(tabId) {
  if (typeof tabId !== "number") return null;
  try {
    const tab = await chrome.tabs.get(tabId);
    if (tab.url) {
      return new URL(tab.url).hostname;
    }
  } catch (_) {
    // Tab may have been closed
  }
  return null;
}

function isSensitiveDomain(domain) {
  if (!domain) return false;
  return SENSITIVE_DOMAIN_PATTERNS.some((pattern) => pattern.test(domain));
}

function extractDomain(url) {
  try {
    return new URL(url).hostname;
  } catch (_) {
    return null;
  }
}

function getEffectiveRiskLevel(method, domain) {
  const baseLevel = CDP_ALLOWLIST[method];
  if (!baseLevel) return null;

  // Elevate L2 to L3 on sensitive domains
  if (baseLevel === 'L2' && isSensitiveDomain(domain)) {
    return 'L3';
  }

  return baseLevel;
}

async function requestL3Confirmation(id, method, domain) {
  // If there's already a pending L3, reject the new one (no queuing)
  if (pendingL3 !== null) {
    return { id, error: { code: -32000, message: `Another L3 confirmation is pending. Try again later.` } };
  }

  l3NonceCounter++;
  const nonce = `l3_${l3NonceCounter}_${Date.now()}`;

  return new Promise((resolve) => {
    const timer = setTimeout(() => {
      pendingL3 = null;
      broadcastL3Status(null);
      resolve({ id, error: { code: -32000, message: `L3 confirmation timed out for ${method}` } });
    }, L3_CONFIRM_TIMEOUT_MS);

    pendingL3 = {
      id,
      method,
      nonce,
      domain: domain || "unknown",
      resolve: (allowed) => {
        clearTimeout(timer);
        pendingL3 = null;
        broadcastL3Status(null);
        if (allowed) {
          resolve(null); // null = proceed with execution
        } else {
          resolve({ id, error: { code: -32000, message: `L3 command ${method} denied by user` } });
        }
      },
    };

    broadcastL3Status({ method, domain: domain || "unknown", nonce });
  });
}

function broadcastL3Status(pending) {
  chrome.runtime
    .sendMessage({
      type: "l3Status",
      pending,
    })
    .catch(() => {
      // Popup not open, ignore
    });
}

async function handleCdpCommand(id, method, params, tabId, sessionId) {
  if (!attachedTabs.has(tabId)) {
    return {
      id,
      error: {
        code: -32000,
        message: `Tab ${tabId} not attached. Call Extension.attachTab first.`,
      },
    };
  }

  // Allowlist check
  if (!(method in CDP_ALLOWLIST)) {
    return {
      id,
      error: { code: -32000, message: `Method not allowed: ${method}` },
    };
  }

  const domain = await getTabDomain(tabId);
  const riskLevel = getEffectiveRiskLevel(method, domain);

  // L2: auto-approve with logging
  if (riskLevel === 'L2') {
    debugLog(`[actionbook] L2 command: ${method} on ${domain || "unknown"} (tab ${tabId})`);
  }

  // L3: require user confirmation
  if (riskLevel === 'L3') {
    debugLog(`[actionbook] L3 command requires confirmation: ${method} on ${domain || "unknown"} (tab ${tabId})`);
    const denial = await requestL3Confirmation(id, method, domain);
    if (denial) return denial;
  }

  try {
    const result = await chrome.debugger.sendCommand(
      {
        tabId,
        ...(typeof sessionId === "string" ? { sessionId } : {}),
      },
      method,
      params
    );
    return { id, result: result || {} };
  } catch (err) {
    const errorMessage = err.message || String(err);

    // Detect debugger detachment (user closed debug banner, tab crashed, etc.)
    if (
      errorMessage.includes("Debugger is not attached") ||
      errorMessage.includes("No tab with given id") ||
      errorMessage.includes("Cannot access") ||
      errorMessage.includes("Target closed")
    ) {
      attachedTabs.delete(tabId);
      broadcastState();
      return {
        id,
        error: {
          code: -32000,
          message: `Debugger detached from tab ${tabId}: ${errorMessage}. Call Extension.attachTab to re-attach.`,
        },
      };
    }

    return {
      id,
      error: { code: -32000, message: errorMessage },
    };
  }
}

// --- State Broadcasting to Popup ---

function broadcastState() {
  chrome.runtime
    .sendMessage({
      type: "stateUpdate",
      connectionState,
      attachedTabIds: Array.from(attachedTabs),
      retryCount,
      maxRetries: MAX_RETRIES,
    })
    .catch(() => {
      // Popup not open, ignore
    });
  updateToolbarIcon();
}

// --- Toolbar icon: black when connected, grey otherwise ---

const ICON_SIZES = [16, 32, 48, 128];
const PLAIN_ICON_PATH = {
  16: "icons/icon-16.png",
  48: "icons/icon-48.png",
  128: "icons/icon-128.png",
};
const GREY_TINT = "#b0b0b0";
const baseBitmapCache = new Map();

async function loadBaseBitmap(size) {
  if (baseBitmapCache.has(size)) return baseBitmapCache.get(size);
  const srcSize = size <= 16 ? 16 : size <= 48 ? 48 : 128;
  const res = await fetch(chrome.runtime.getURL(`icons/icon-${srcSize}.png`));
  const blob = await res.blob();
  const bitmap = await createImageBitmap(blob);
  baseBitmapCache.set(size, bitmap);
  return bitmap;
}

// Re-tint the black logo to grey by using it as an alpha mask and filling
// with a grey color. source-in keeps only the alpha of the existing pixels.
async function renderGreyIcon(size) {
  const canvas = new OffscreenCanvas(size, size);
  const ctx = canvas.getContext("2d");
  const bitmap = await loadBaseBitmap(size);
  ctx.drawImage(bitmap, 0, 0, size, size);
  ctx.globalCompositeOperation = "source-in";
  ctx.fillStyle = GREY_TINT;
  ctx.fillRect(0, 0, size, size);
  return ctx.getImageData(0, 0, size, size);
}

async function updateToolbarIcon() {
  try {
    if (connectionState === "connected") {
      chrome.action.setIcon({ path: PLAIN_ICON_PATH });
      return;
    }
    const imageData = {};
    for (const size of ICON_SIZES) {
      imageData[size] = await renderGreyIcon(size);
    }
    chrome.action.setIcon({ imageData });
  } catch (_) {
    try { chrome.action.setIcon({ path: PLAIN_ICON_PATH }); } catch (_) {}
  }
}

// Validate that a message sender is the extension's own popup
function isSenderPopup(sender) {
  return (
    sender.id === chrome.runtime.id &&
    sender.url &&
    sender.url.includes("popup.html")
  );
}

// Validate that a message sender is the extension's own callback page
// (callback.html is loaded from chrome-extension://<id>/callback.html after
// the OAuth module redirects the user back).
function isSenderCallback(sender) {
  return (
    sender.id === chrome.runtime.id &&
    sender.url &&
    sender.url.includes("callback.html")
  );
}

// Listen for messages from popup and offscreen document
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message.type === "getState") {
    sendResponse({
      connectionState,
      attachedTabIds: Array.from(attachedTabs),
      retryCount,
      maxRetries: MAX_RETRIES,
    });
    return true;
  }
  if (message.type === "connect") {
    // User-initiated connection from popup
    if (connectionState === "idle" || connectionState === "disconnected" || connectionState === "failed" || connectionState === "pairing_required") {
      wasReplaced = false;
      retryCount = 0;
      reconnectDelay = RECONNECT_BASE_MS;
      startBridgePolling();
      connect();
    }
    return false;
  }
  if (message.type === "retry") {
    // User-initiated retry (reset retry count and reconnect)
    wasReplaced = false;
    retryCount = 0;
    reconnectDelay = RECONNECT_BASE_MS;
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    startBridgePolling();
    connect();
    return false;
  }
  if (message.type === "setToken") {
    // Backward-compatible no-op: token is no longer used.
    if (!isSenderPopup(sender)) return false;
    retryCount = 0;
    reconnectDelay = RECONNECT_BASE_MS;
    startBridgePolling();
    connect();
    return false;
  }
  if (message.type === "getL3Status") {
    sendResponse({ pending: pendingL3 ? { method: pendingL3.method, domain: pendingL3.domain } : null });
    return true;
  }
  if (message.type === "l3Response") {
    // Only accept L3 responses from our own popup with matching nonce
    if (!isSenderPopup(sender)) return false;
    if (pendingL3 && pendingL3.resolve && message.nonce === pendingL3.nonce) {
      pendingL3.resolve(message.allowed === true);
    }
    return false;
  }
  if (message.type === "keepalive") {
    // Offscreen document keep-alive ping - just acknowledge
    return false;
  }
  if (message.type === "getGrouping") {
    // Match the setter's sender check — only the popup reads this state.
    if (!isSenderPopup(sender)) return false;
    sendResponse({ enabled: groupingEnabled });
    return true;
  }
  if (message.type === "setGrouping") {
    // Only trust the popup — other senders cannot flip grouping
    if (!isSenderPopup(sender)) return false;
    groupingEnabled = message.enabled === true;
    chrome.storage.local.set({ groupTabs: groupingEnabled });
    return false;
  }
  // Cloud mode: popup asks to switch between local / cloud. We close the
  // current WS and reconnect with the new config.
  if (message.type === "setMode") {
    if (!isSenderPopup(sender)) return false;
    const mode = message.mode === "cloud" ? "cloud" : "local";
    chrome.storage.local.set({ mode }, () => {
      wasReplaced = false;
      retryCount = 0;
      reconnectDelay = RECONNECT_BASE_MS;
      if (reconnectTimer) {
        clearTimeout(reconnectTimer);
        reconnectTimer = null;
      }
      // Must detach handlers before closing — otherwise the old socket's
      // onclose races with the new connection below.
      detachAndCloseWs("mode_switch");
      connect();
    });
    return false;
  }
  // Cloud mode: callback.html received a token from the OAuth redirect and
  // stashed it into storage. Trigger a reconnect so we pick it up.
  if (message.type === "cloud_auth_updated") {
    if (!isSenderCallback(sender) && !isSenderPopup(sender)) return false;
    wasReplaced = false;
    retryCount = 0;
    reconnectDelay = RECONNECT_BASE_MS;
    detachAndCloseWs("token_updated");
    connect();
    return false;
  }
  // Cloud mode: popup asks to sign out — clear token + refresh token + expiry
  // and disconnect so a subsequent Sign in starts a fresh OAuth flow.
  if (message.type === "cloud_sign_out") {
    if (!isSenderPopup(sender)) return false;
    chrome.storage.local.remove(
      ["cloudToken", "cloudRefreshToken", "cloudTokenExpiresAt"],
      () => {
        detachAndCloseWs("sign_out");
        connectionState = "pairing_required";
        logStateTransition("pairing_required", "user signed out");
        broadcastState();
      },
    );
    return false;
  }
  return false;
});

// Clean up debugger state when a tab is closed.
chrome.tabs.onRemoved.addListener((tabId) => {
  if (attachedTabs.delete(tabId)) {
    broadcastState();
  }
});

// Handle debugger detach events (user cancelled the debug banner, tab crashed,
// etc.). Remove only the affected tab from the attached set.
chrome.debugger.onDetach.addListener((source, reason) => {
  const tabId = source.tabId;
  if (typeof tabId === "number" && attachedTabs.has(tabId)) {
    debugLog(`[actionbook] Debugger detached from tab ${tabId}: ${reason}`);
    attachedTabs.delete(tabId);
    broadcastState();
  }
});

// --- Fixed bridge polling (no Native Messaging) ---

const BRIDGE_POLL_INTERVAL_MS = 2000;
let bridgePollTimer = null;

function startBridgePolling() {
  if (bridgePollTimer) return;
  bridgePollTimer = setInterval(() => {
    if (connectionState === "connected" || connectionState === "connecting") return;
    connect();
  }, BRIDGE_POLL_INTERVAL_MS);
}

function stopBridgePolling() {
  if (bridgePollTimer) {
    clearInterval(bridgePollTimer);
    bridgePollTimer = null;
  }
}

// --- Start ---

ensureOffscreenDocument();
lastLoggedState = "idle";
debugLog("[actionbook] Background service worker started");

// One-shot migration: pre-cloud-default installs (v0.5.x and earlier) ran on
// local mode without ever writing chrome.storage.local.mode. Flipping the
// runtime default to "cloud" would silently pull those users into
// pairing_required and break their working local CLI bridge until they
// manually switch back. The migration runs at most once per profile —
// `_modeMigratedToCloudDefault` records that we've already evaluated this
// install, so future cloud-default → next-version updates with an unset mode
// are correctly left on cloud instead of being re-pinned to local.
chrome.runtime.onInstalled.addListener(async (details) => {
  const { mode, _modeMigratedToCloudDefault } = await chrome.storage.local.get([
    "mode",
    "_modeMigratedToCloudDefault",
  ]);
  if (_modeMigratedToCloudDefault) return;
  await chrome.storage.local.set({ _modeMigratedToCloudDefault: true });
  // Only updates with no explicit mode are the legacy local-default cohort.
  // Fresh installs (reason="install") and users who already chose a mode fall
  // through to the cloud default.
  if (details.reason !== "update") return;
  if (mode) return;
  await chrome.storage.local.set({ mode: "local" });
  // The startup connect() below may have already raced against the cloud
  // default. Tear it down and reconnect against the freshly pinned local mode.
  wasReplaced = false;
  retryCount = 0;
  reconnectDelay = RECONNECT_BASE_MS;
  if (reconnectTimer) {
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }
  detachAndCloseWs("mode_migration");
  connect();
});

// Load the user's tab-grouping preference (default on). Kept fully async:
// the few grouping calls that might race this load will just see the default
// value, which is the safer fallback.
chrome.storage.local.get("groupTabs", (result) => {
  if (typeof result?.groupTabs === "boolean") {
    groupingEnabled = result.groupTabs;
  }
});

// Sync the toolbar icon to the current connectionState — Chrome persists
// setIcon across service-worker restarts, so a stale state could linger.
updateToolbarIcon();

// Try immediate connect, then keep polling fixed ws://127.0.0.1:19222.
connect();
startBridgePolling();

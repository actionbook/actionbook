// Actionbook Browser Bridge - Background Service Worker
// Connects to the CLI bridge server via WebSocket and executes browser commands

const BRIDGE_URL = "ws://127.0.0.1:19222";
const RECONNECT_BASE_MS = 1000;
const RECONNECT_MAX_MS = 30000;
const MAX_RETRIES = 8;
const BRIDGE_PROBE_TIMEOUT_MS = 750;

const HANDSHAKE_TIMEOUT_MS = 2000;
const L3_CONFIRM_TIMEOUT_MS = 30000;

// --- Tab Group Config ---
// All tabs opened by Actionbook (via Extension.createTab or the reuse-empty-tab
// path) are moved into a per-window Chrome tab group titled "Actionbook" so
// users can tell agent-driven tabs apart at a glance and collapse/close them
// in bulk. The group is looked up by title in the tab's own window — we do
// NOT persist groupId, since it's unstable across sessions and windows.
const ACTIONBOOK_GROUP_TITLE = "Actionbook";
const ACTIONBOOK_GROUP_COLOR = "grey";
// When true, tabs that the user explicitly attaches via Extension.attachTab
// are also moved into the group. Default false: don't yank a user's existing
// tab into the Actionbook group without their knowledge.
const ACTIONBOOK_GROUP_ATTACH = false;
// User-facing toggle (chrome.storage.local key: "groupTabs"). Default on.
let groupingEnabled = true;

// --- CDP Method Allowlist ---

const CDP_ALLOWLIST = {
  // L1 - Read only (auto-approved)
  'Page.captureScreenshot': 'L1',
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

async function getEffectiveBridgeUrl() {
  return BRIDGE_URL;
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

  try {
    const bridgeUrl = await getEffectiveBridgeUrl();
    if (!(await canReachBridge(bridgeUrl))) {
      connectionState = "disconnected";
      logStateTransition("disconnected", "bridge not listening");
      broadcastState();
      scheduleReconnect();
      return;
    }
    ws = new WebSocket(bridgeUrl);
  } catch (err) {
    connectionState = "disconnected";
    logStateTransition("disconnected", "WebSocket constructor error");
    broadcastState();
    scheduleReconnect();
    return;
  }

  handshakeCompleted = false;
  let wsOpened = false;

  ws.onopen = () => {
    wsOpened = true;
    // Send hello handshake (tokenless - server validates via origin + extension ID).
    // Protocol 0.4.0 narrows `Extension.listTabs` to Actionbook-managed tabs
    // (debugger-attached OR in the "Actionbook" tab group), making session
    // ownership first-class instead of returning every open Chrome tab.
    wsSend({
      type: "hello",
      role: "extension",
      version: "0.4.0",
    });

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
      if (ws) { ws.close(); ws = null; }

      if (msg.error === "invalid_token") {
        connectionState = "disconnected";
        logStateTransition("disconnected", "invalid token");
        broadcastState();
        startBridgePolling();
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
    });
  }
});

// --- Tab Group Helper ---

// Move a tab into the Actionbook group in its current window, creating the
// group if none exists there. All failures are swallowed: grouping is a UX
// nicety and must never break the CDP command that triggered it. Callers
// should not await any particular outcome — treat this as fire-and-log.
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
  const { id, method, params, tabId } = msg;

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
    return await handleCdpCommand(id, method, params || {}, tabId);
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
      // Only return tabs that Actionbook is actually managing:
      //   * tabs currently in the "Actionbook" tab group (any window), OR
      //   * tabs the extension has debugger-attached (`attachedTabs`).
      // Union, not intersection: user may have disabled grouping
      // (groupingEnabled=false) so attached tabs won't be in any group, and
      // ACTIONBOOK_GROUP_ATTACH=false means user-attached existing tabs are
      // not moved into the group. Either signal is enough to claim the tab.
      let actionbookGroupIds = new Set();
      if (chrome.tabGroups && chrome.tabGroups.query) {
        try {
          const groups = await chrome.tabGroups.query({
            title: ACTIONBOOK_GROUP_TITLE,
          });
          actionbookGroupIds = new Set(groups.map((g) => g.id));
        } catch (_) {
          // tabGroups unavailable — fall back to attachedTabs only.
        }
      }
      const all = await chrome.tabs.query({});
      const managed = all.filter(
        (t) =>
          attachedTabs.has(t.id) ||
          (typeof t.groupId === "number" && actionbookGroupIds.has(t.groupId))
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
      // Opt-in: only move user-attached existing tabs into the group when
      // ACTIONBOOK_GROUP_ATTACH is true. Default false preserves user intent.
      if (ACTIONBOOK_GROUP_ATTACH) {
        await ensureTabInActionbookGroup(tabId);
      }
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

async function handleCdpCommand(id, method, params, tabId) {
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
      { tabId },
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
}

// Validate that a message sender is the extension's own popup
function isSenderPopup(sender) {
  return (
    sender.id === chrome.runtime.id &&
    sender.url &&
    sender.url.includes("popup.html")
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

// Load the user's tab-grouping preference (default on). Kept fully async:
// the few grouping calls that might race this load will just see the default
// value, which is the safer fallback.
chrome.storage.local.get("groupTabs", (result) => {
  if (typeof result?.groupTabs === "boolean") {
    groupingEnabled = result.groupTabs;
  }
});

// Try immediate connect, then keep polling fixed ws://127.0.0.1:19222.
connect();
startBridgePolling();

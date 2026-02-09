// Actionbook Browser Bridge - Background Service Worker
// Connects to the CLI bridge server via WebSocket and executes browser commands

const DEFAULT_BRIDGE_PORT = 19222;
const RECONNECT_BASE_MS = 1000;
const RECONNECT_MAX_MS = 30000;
const MAX_RETRIES = 8;

const HANDSHAKE_TIMEOUT_MS = 2000;
const L3_CONFIRM_TIMEOUT_MS = 30000;

// --- CDP Method Allowlist ---

const CDP_ALLOWLIST = {
  // L1 - Read only (auto-approved)
  'Page.captureScreenshot': 'L1',
  'DOM.getDocument': 'L1',
  'DOM.querySelector': 'L1',
  'DOM.querySelectorAll': 'L1',
  'DOM.getOuterHTML': 'L1',
  'Network.getCookies': 'L1',

  // L2 - Page modification (auto-approved with logging)
  'Runtime.evaluate': 'L2',
  'Page.navigate': 'L2',
  'Page.reload': 'L2',
  'Input.dispatchMouseEvent': 'L2',
  'Input.dispatchKeyEvent': 'L2',
  'Emulation.setDeviceMetricsOverride': 'L2',
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
let attachedTabId = null;
let connectionState = "idle"; // idle | pairing_required | connecting | connected | disconnected | failed
let reconnectDelay = RECONNECT_BASE_MS;
let reconnectTimer = null;
let retryCount = 0;
let lastLoggedState = null;
let handshakeTimer = null;
let handshakeCompleted = false;

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

const TOKEN_PREFIX = "abk_";
const TOKEN_EXPECTED_LENGTH = 36; // "abk_" (4) + 32 hex chars

function isValidTokenFormat(token) {
  return (
    typeof token === "string" &&
    token.startsWith(TOKEN_PREFIX) &&
    token.length === TOKEN_EXPECTED_LENGTH &&
    /^[0-9a-f]+$/.test(token.slice(TOKEN_PREFIX.length))
  );
}

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

async function getStoredToken() {
  return new Promise((resolve) => {
    chrome.storage.local.get("bridgeToken", (result) => {
      resolve(result.bridgeToken || null);
    });
  });
}

async function getEffectiveBridgeUrl() {
  return new Promise((resolve) => {
    chrome.storage.local.get("bridgePort", (result) => {
      const port = result.bridgePort || DEFAULT_BRIDGE_PORT;
      resolve(`ws://localhost:${port}`);
    });
  });
}

async function connect() {
  if (ws && ws.readyState === WebSocket.OPEN) return;
  if (connectionState === "connecting") return;

  const token = await getStoredToken();
  if (!token) {
    connectionState = "pairing_required";
    logStateTransition("pairing_required", "no token stored");
    broadcastState();
    return;
  }

  connectionState = "connecting";
  logStateTransition("connecting");
  broadcastState();

  try {
    const bridgeUrl = await getEffectiveBridgeUrl();
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
    // Send hello handshake with token
    wsSend({
      type: "hello",
      role: "extension",
      token: token,
      version: "0.2.0",
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
      stopNativePolling();
      logStateTransition("connected");
      broadcastState();
      return;
    }

    // Handle token_expired from server (token rotated due to inactivity)
    if (msg.type === "token_expired") {
      chrome.storage.local.remove("bridgeToken", () => {
        connectionState = "pairing_required";
        logStateTransition("pairing_required", "token expired by server");
        broadcastState();
        startNativePolling();
      });
      if (ws) { ws.close(); ws = null; }
      return;
    }

    // Handle hello_error from server (version mismatch, etc.)
    if (msg.type === "hello_error") {
      debugLog("[actionbook] Server rejected handshake:", msg.message);
      handshakeCompleted = false;
      if (handshakeTimer) {
        clearTimeout(handshakeTimer);
        handshakeTimer = null;
      }
      connectionState = "failed";
      logStateTransition("failed", msg.message || "handshake rejected by server");
      broadcastState();
      if (ws) { ws.close(); ws = null; }
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

    if (!handshakeCompleted) {
      if (!wsOpened) {
        // Connection never opened - network error (server down, etc.)
        connectionState = "disconnected";
        logStateTransition("disconnected", "connection refused (server not running?)");
        broadcastState();
        scheduleReconnect();
      } else {
        // Connection opened but handshake rejected - auth failure, token may have rotated
        // Clear stored token and restart native messaging polling to get new token
        chrome.storage.local.remove("bridgeToken", () => {
          connectionState = "pairing_required";
          logStateTransition("pairing_required", "handshake failed (bad token?), cleared stored token");
          broadcastState();
          startNativePolling();
        });
      }
      return;
    }

    // Was connected, now disconnected - bridge may have stopped
    // Restart native polling to auto-reconnect when bridge comes back with new token
    chrome.storage.local.remove("bridgeToken", () => {
      connectionState = "disconnected";
      logStateTransition("disconnected", "bridge connection lost");
      broadcastState();
      startNativePolling();
    });
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
  if (source.tabId === attachedTabId) {
    // Forward CDP event to bridge (no id field = event, not response)
    wsSend({
      method: method,
      params: params || {},
    });
  }
});

// --- Command Handler ---

async function handleCommand(msg) {
  const { id, method, params } = msg;

  if (!method) {
    return { id, error: { code: -32600, message: "Missing method" } };
  }

  try {
    // Extension-specific commands (non-CDP)
    if (method.startsWith("Extension.")) {
      return await handleExtensionCommand(id, method, params || {});
    }

    // CDP commands - forward to chrome.debugger
    return await handleCdpCommand(id, method, params || {});
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
      const tabs = await chrome.tabs.query({});
      const tabList = tabs.map((t) => ({
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

      // Verify tab exists
      try {
        await chrome.tabs.get(tabId);
      } catch (_) {
        return { id, error: { code: -32000, message: `Tab ${tabId} not found` } };
      }

      // Detach from current tab if any
      if (attachedTabId !== null) {
        try {
          await chrome.debugger.detach({ tabId: attachedTabId });
        } catch (_) {
          // Ignore detach errors
        }
      }

      await chrome.debugger.attach({ tabId }, "1.3");
      attachedTabId = tabId;
      return { id, result: { attached: true, tabId } };
    }

    case "Extension.attachActiveTab": {
      const [tab] = await chrome.tabs.query({
        active: true,
        currentWindow: true,
      });
      if (!tab) {
        return { id, error: { code: -32000, message: "No active tab found" } };
      }

      if (attachedTabId !== null && attachedTabId !== tab.id) {
        try {
          await chrome.debugger.detach({ tabId: attachedTabId });
        } catch (_) {
          // Ignore
        }
      }

      await chrome.debugger.attach({ tabId: tab.id }, "1.3");
      attachedTabId = tab.id;
      return { id, result: { attached: true, tabId: tab.id, title: tab.title, url: tab.url } };
    }

    case "Extension.createTab": {
      const url = params.url || "about:blank";
      const tab = await chrome.tabs.create({ url });

      // Auto-attach debugger to the new tab so subsequent CDP commands target it
      try {
        if (attachedTabId !== null && attachedTabId !== tab.id) {
          try { await chrome.debugger.detach({ tabId: attachedTabId }); } catch (_) {}
        }
        if (attachedTabId !== tab.id) {
          await chrome.debugger.attach({ tabId: tab.id }, "1.3");
          attachedTabId = tab.id;
        }
        return { id, result: { tabId: tab.id, title: tab.title || "", url: tab.url || url, attached: true } };
      } catch (err) {
        // Tab created but debugger attach failed — return tab info with attached: false
        return { id, result: { tabId: tab.id, title: tab.title || "", url: tab.url || url, attached: false, attachError: err.message } };
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

        // Auto-attach debugger to the activated tab so subsequent CDP commands target it
        if (attachedTabId !== null && attachedTabId !== tabId) {
          try { await chrome.debugger.detach({ tabId: attachedTabId }); } catch (_) {}
        }
        if (attachedTabId !== tabId) {
          await chrome.debugger.attach({ tabId }, "1.3");
          attachedTabId = tabId;
        }

        return { id, result: { success: true, tabId, title: tab.title, url: tab.url, attached: true } };
      } catch (err) {
        return { id, error: { code: -32000, message: `Failed to activate tab ${tabId}: ${err.message}` } };
      }
    }

    case "Extension.detachTab": {
      if (attachedTabId === null) {
        return { id, result: { detached: true } };
      }
      try {
        await chrome.debugger.detach({ tabId: attachedTabId });
      } catch (_) {
        // Ignore
      }
      attachedTabId = null;
      return { id, result: { detached: true } };
    }

    case "Extension.status": {
      return {
        id,
        result: {
          connected: connectionState === "connected",
          attachedTabId,
          version: "0.2.0",
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

async function getAttachedTabDomain() {
  if (attachedTabId === null) return null;
  try {
    const tab = await chrome.tabs.get(attachedTabId);
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

async function handleCdpCommand(id, method, params) {
  if (attachedTabId === null) {
    return {
      id,
      error: {
        code: -32000,
        message: "No tab attached. Use Extension.attachTab first.",
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

  const domain = await getAttachedTabDomain();
  const riskLevel = getEffectiveRiskLevel(method, domain);

  // L2: auto-approve with logging
  if (riskLevel === 'L2') {
    debugLog(`[actionbook] L2 command: ${method} on ${domain || "unknown"}`);
  }

  // L3: require user confirmation
  if (riskLevel === 'L3') {
    debugLog(`[actionbook] L3 command requires confirmation: ${method} on ${domain || "unknown"}`);
    const denial = await requestL3Confirmation(id, method, domain);
    if (denial) return denial;
  }

  try {
    const result = await chrome.debugger.sendCommand(
      { tabId: attachedTabId },
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
      const previousTabId = attachedTabId;
      attachedTabId = null;
      broadcastState();
      return {
        id,
        error: {
          code: -32000,
          message: `Debugger detached from tab ${previousTabId}: ${errorMessage}. Call Extension.attachTab to re-attach.`,
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
      attachedTabId,
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
      attachedTabId,
      retryCount,
      maxRetries: MAX_RETRIES,
    });
    return true;
  }
  if (message.type === "connect") {
    // User-initiated connection from popup
    if (connectionState === "idle" || connectionState === "disconnected" || connectionState === "failed" || connectionState === "pairing_required") {
      retryCount = 0;
      reconnectDelay = RECONNECT_BASE_MS;
      nativeMessagingAvailable = true;
      nativeMessagingFailCount = 0;
      if (nativeRecheckTimer) { clearTimeout(nativeRecheckTimer); nativeRecheckTimer = null; }
      // Try native messaging first, then start polling if needed
      tryNativeMessagingConnect();
      startNativePolling();
    }
    return false;
  }
  if (message.type === "retry") {
    // User-initiated retry (reset retry count and reconnect)
    retryCount = 0;
    reconnectDelay = RECONNECT_BASE_MS;
    nativeMessagingAvailable = true;
    nativeMessagingFailCount = 0;
    if (nativeRecheckTimer) { clearTimeout(nativeRecheckTimer); nativeRecheckTimer = null; }
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    connect();
    return false;
  }
  if (message.type === "setToken") {
    // Only accept token changes from our own popup
    if (!isSenderPopup(sender)) return false;
    const token = (message.token || "").trim();
    if (!isValidTokenFormat(token)) {
      debugLog("[actionbook] Rejected invalid token format");
      return false;
    }
    chrome.storage.local.set({ bridgeToken: token }, () => {
      retryCount = 0;
      reconnectDelay = RECONNECT_BASE_MS;
      connect();
    });
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
  return false;
});

// Clean up debugger on tab close
chrome.tabs.onRemoved.addListener((tabId) => {
  if (tabId === attachedTabId) {
    attachedTabId = null;
    broadcastState();
  }
});

// Handle debugger detach events
chrome.debugger.onDetach.addListener((source, reason) => {
  if (source.tabId === attachedTabId) {
    debugLog(`[actionbook] Debugger detached from tab ${attachedTabId}: ${reason}`);
    attachedTabId = null;
    broadcastState();
  }
});

// --- Native Messaging: Auto-connect ---

const NATIVE_HOST_NAME = "com.actionbook.bridge";

// How often to poll native messaging when not connected (ms)
const NATIVE_POLL_INTERVAL_MS = 5000;
let nativePollTimer = null;
let nativeMessagingAvailable = true; // assume available until proven otherwise
let nativeMessagingFailCount = 0;
const NATIVE_MESSAGING_MAX_FAILS = 5;
let nativeRecheckTimer = null;

/**
 * Attempt to read the bridge session token from the CLI via Chrome Native Messaging.
 * If successful, stores the token and initiates connection automatically.
 * Falls back silently to manual token entry if the native host is not installed.
 */
async function tryNativeMessagingConnect() {
  // Skip if already connected or connecting
  if (connectionState === "connected" || connectionState === "connecting") return;

  // If native messaging was determined to be unavailable, don't retry
  if (!nativeMessagingAvailable) return;

  const existingToken = await getStoredToken();
  if (existingToken) {
    // Already have a token - just connect directly
    connect();
    return;
  }

  try {
    chrome.runtime.sendNativeMessage(
      NATIVE_HOST_NAME,
      { type: "get_token" },
      (response) => {
        if (chrome.runtime.lastError) {
          const errMsg = chrome.runtime.lastError.message || "";
          // "Specified native messaging host not found" means host not installed
          if (errMsg.includes("not found") || errMsg.includes("not registered")) {
            nativeMessagingFailCount++;
            debugLog(`[actionbook] Native messaging unavailable (attempt ${nativeMessagingFailCount}/${NATIVE_MESSAGING_MAX_FAILS})`);
            if (nativeMessagingFailCount >= NATIVE_MESSAGING_MAX_FAILS) {
              nativeMessagingAvailable = false;
              stopNativePolling();
              scheduleNativeRecheck();
            }
          } else {
            debugLog("[actionbook] Native messaging error:", errMsg);
          }
          return;
        }

        if (response && response.type === "token" && response.token && response.bridge_running) {
          debugLog("[actionbook] Token received via native messaging");
          if (!isValidTokenFormat(response.token)) {
            debugLog("[actionbook] Rejected invalid token from native host");
            return;
          }
          const storageData = { bridgeToken: response.token };
          if (response.port) {
            storageData.bridgePort = response.port;
          }
          chrome.storage.local.set(storageData, () => {
            retryCount = 0;
            reconnectDelay = RECONNECT_BASE_MS;
            stopNativePolling();
            connect();
          });
        } else if (response && response.type === "error" && response.error === "no_token") {
          // Bridge not running yet - keep polling
          debugLog("[actionbook] Bridge not running, will retry...");
        }
      }
    );
  } catch (err) {
    debugLog("[actionbook] Native messaging error:", err);
  }
}

/**
 * Schedule a periodic recheck of native messaging availability.
 * After too many failures, we pause polling but recheck every 60s.
 */
function scheduleNativeRecheck() {
  if (nativeRecheckTimer) return;
  nativeRecheckTimer = setTimeout(() => {
    nativeRecheckTimer = null;
    nativeMessagingAvailable = true;
    nativeMessagingFailCount = 0;
    startNativePolling();
  }, 60000);
}

/**
 * Start polling native messaging for token when bridge is not yet running.
 * Polls every NATIVE_POLL_INTERVAL_MS until connected or native host unavailable.
 */
function startNativePolling() {
  if (nativePollTimer) return;
  nativePollTimer = setInterval(() => {
    if (connectionState === "connected" || connectionState === "connecting") {
      stopNativePolling();
      return;
    }
    tryNativeMessagingConnect();
  }, NATIVE_POLL_INTERVAL_MS);
}

function stopNativePolling() {
  if (nativePollTimer) {
    clearInterval(nativePollTimer);
    nativePollTimer = null;
  }
}

// --- CDP-injected token listener (isolated mode) ---
// When the CLI injects bridgeToken via chrome.storage.local.set() over CDP,
// this listener fires and triggers an immediate connect(), bypassing native messaging.
chrome.storage.onChanged.addListener((changes, areaName) => {
  if (areaName !== "local") return;
  if (!changes.bridgeToken?.newValue) return;
  if (!isValidTokenFormat(changes.bridgeToken.newValue)) return;
  debugLog("[actionbook] Token injected via storage, connecting...");
  stopNativePolling();
  retryCount = 0;
  reconnectDelay = RECONNECT_BASE_MS;
  connect();
});

// --- Start ---

ensureOffscreenDocument();
lastLoggedState = "idle";
debugLog("[actionbook] Background service worker started");
// Try immediate connect, then start polling if bridge not yet running
tryNativeMessagingConnect();
startNativePolling();

// Actionbook Browser Bridge - Background Service Worker
// Connects to the CLI bridge server via WebSocket and executes browser commands

const BRIDGE_URL = "ws://localhost:19222";
const RECONNECT_BASE_MS = 1000;
const RECONNECT_MAX_MS = 30000;

let ws = null;
let attachedTabId = null;
let connectionState = "disconnected"; // disconnected | connecting | connected
let reconnectDelay = RECONNECT_BASE_MS;
let reconnectTimer = null;

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
    console.log("[actionbook] Offscreen document created for keep-alive");
  } catch (err) {
    // Document may already exist from a race condition
    if (!err.message?.includes("Only a single offscreen")) {
      console.error("[actionbook] Failed to create offscreen document:", err);
    }
  }
}

// --- WebSocket Connection Management ---

function connect() {
  if (ws && ws.readyState === WebSocket.OPEN) return;
  if (connectionState === "connecting") return;

  connectionState = "connecting";
  broadcastState();

  try {
    ws = new WebSocket(BRIDGE_URL);
  } catch (err) {
    console.error("[actionbook] WebSocket constructor error:", err);
    connectionState = "disconnected";
    broadcastState();
    scheduleReconnect();
    return;
  }

  ws.onopen = () => {
    console.log("[actionbook] Connected to bridge server");
    connectionState = "connected";
    reconnectDelay = RECONNECT_BASE_MS; // Reset backoff on successful connection
    broadcastState();

    // Identify as extension client
    wsSend({
      type: "extension",
      version: "0.1.0",
    });
  };

  ws.onmessage = async (event) => {
    let msg;
    try {
      msg = JSON.parse(event.data);
    } catch (err) {
      console.error("[actionbook] Invalid JSON from bridge:", event.data);
      return;
    }

    const response = await handleCommand(msg);
    wsSend(response);
  };

  ws.onclose = () => {
    console.log("[actionbook] Disconnected from bridge server");
    ws = null;
    connectionState = "disconnected";
    broadcastState();
    scheduleReconnect();
  };

  ws.onerror = (err) => {
    console.error("[actionbook] WebSocket error:", err);
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

  console.log(`[actionbook] Reconnecting in ${reconnectDelay}ms...`);
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    connect();
  }, reconnectDelay);

  // Exponential backoff: double delay, cap at max
  reconnectDelay = Math.min(reconnectDelay * 2, RECONNECT_MAX_MS);
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
      return { id, result: { tabId: tab.id, title: tab.title || "", url: tab.url || url } };
    }

    case "Extension.activateTab": {
      const tabId = params.tabId;
      if (!tabId || typeof tabId !== "number") {
        return { id, error: { code: -32602, message: "Missing or invalid tabId" } };
      }
      try {
        await chrome.tabs.update(tabId, { active: true });
        const tab = await chrome.tabs.get(tabId);
        return { id, result: { success: true, tabId, title: tab.title, url: tab.url } };
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
          version: "0.1.0",
        },
      };
    }

    default:
      return {
        id,
        error: { code: -32601, message: `Unknown extension method: ${method}` },
      };
  }
}

async function handleCdpCommand(id, method, params) {
  if (attachedTabId === null) {
    // Auto-attach to active tab if none attached
    const [activeTab] = await chrome.tabs.query({ active: true, currentWindow: true });
    if (!activeTab) {
      return {
        id,
        error: {
          code: -32000,
          message: "No tab attached and no active tab available. Call Extension.attachTab or Extension.attachActiveTab first.",
        },
      };
    }

    try {
      await chrome.debugger.attach({ tabId: activeTab.id }, "1.3");
      attachedTabId = activeTab.id;
      console.log(`[actionbook] Auto-attached to active tab ${activeTab.id}: ${activeTab.title}`);
      broadcastState();
    } catch (attachErr) {
      return {
        id,
        error: {
          code: -32000,
          message: `Failed to auto-attach to active tab: ${attachErr.message || String(attachErr)}`,
        },
      };
    }
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
    })
    .catch(() => {
      // Popup not open, ignore
    });
}

// Listen for messages from popup and offscreen document
chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (message.type === "getState") {
    sendResponse({
      connectionState,
      attachedTabId,
    });
    return true;
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
    console.log(`[actionbook] Debugger detached from tab ${attachedTabId}: ${reason}`);
    attachedTabId = null;
    broadcastState();
  }
});

// --- Start ---

ensureOffscreenDocument();
connect();
console.log("[actionbook] Background service worker started");

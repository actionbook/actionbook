// Actionbook Browser Bridge - Background Service Worker
// Connects to the CLI bridge server via WebSocket and executes browser commands

const BRIDGE_URL = "ws://localhost:19222";
const RECONNECT_INTERVAL_MS = 5000;

let ws = null;
let attachedTabId = null;
let connectionState = "disconnected"; // disconnected | connecting | connected

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
  setTimeout(connect, RECONNECT_INTERVAL_MS);
}

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
    return {
      id,
      error: {
        code: -32000,
        message: "No tab attached. Call Extension.attachTab or Extension.attachActiveTab first.",
      },
    };
  }

  const result = await chrome.debugger.sendCommand(
    { tabId: attachedTabId },
    method,
    params
  );

  return { id, result: result || {} };
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

// Listen for messages from popup
chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (message.type === "getState") {
    sendResponse({
      connectionState,
      attachedTabId,
    });
    return true;
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

connect();
console.log("[actionbook] Background service worker started");

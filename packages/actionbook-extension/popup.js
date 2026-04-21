// Popup script - displays connection status and provides connect/retry/token/L3 confirmation controls

function updateUI(state) {
  const bridgeDot = document.getElementById("bridgeDot");
  const bridgeStatus = document.getElementById("bridgeStatus");
  const tabDot = document.getElementById("tabDot");
  const tabStatus = document.getElementById("tabStatus");
  const retryInfo = document.getElementById("retryInfo");
  const actionBtn = document.getElementById("actionBtn");
  const tokenSection = document.getElementById("tokenSection");

  // Reset UI elements
  actionBtn.classList.add("hidden");
  tokenSection.classList.add("hidden");
  retryInfo.textContent = "";

  switch (state.connectionState) {
    case "connected":
      bridgeDot.className = "dot green";
      bridgeStatus.textContent = "Connected";
      break;
    case "connecting":
      bridgeDot.className = "dot yellow";
      bridgeStatus.textContent = "Connecting...";
      if (state.retryCount > 0) {
        retryInfo.textContent = `Attempt ${state.retryCount}/${state.maxRetries}`;
      }
      break;
    case "disconnected":
      bridgeDot.className = "dot orange";
      bridgeStatus.textContent = "Disconnected";
      if (state.retryCount > 0) {
        retryInfo.textContent = `Attempt ${state.retryCount}/${state.maxRetries}`;
      }
      actionBtn.textContent = "Connect";
      actionBtn.classList.remove("hidden");
      break;
    case "failed":
      bridgeDot.className = "dot red";
      bridgeStatus.textContent = "Connection failed";
      retryInfo.textContent = "retries exhausted";
      actionBtn.textContent = "Retry";
      actionBtn.classList.remove("hidden");
      break;
    case "pairing_required":
      // Deprecated: Token no longer required, treat as disconnected
      bridgeDot.className = "dot orange";
      bridgeStatus.textContent = "Waiting for bridge";
      actionBtn.textContent = "Connect";
      actionBtn.classList.remove("hidden");
      break;
    case "idle":
    default:
      bridgeDot.className = "dot gray";
      bridgeStatus.textContent = "Not connected";
      // Note: Token section hidden by default (tokenless mode)
      actionBtn.textContent = "Connect";
      actionBtn.classList.remove("hidden");
      break;
  }

  if (state.attachedTabId) {
    tabDot.className = "dot green";
    tabStatus.textContent = `Tab #${state.attachedTabId}`;
  } else {
    tabDot.className = "dot gray";
    tabStatus.textContent = "No tab attached";
  }
}

let currentL3Nonce = null;

function updateL3UI(pending) {
  const confirmSection = document.getElementById("confirmSection");
  const confirmMethod = document.getElementById("confirmMethod");
  const confirmDomain = document.getElementById("confirmDomain");

  if (pending) {
    currentL3Nonce = pending.nonce || null;
    confirmMethod.textContent = pending.method;
    confirmDomain.textContent = pending.domain;
    confirmSection.classList.remove("hidden");
  } else {
    currentL3Nonce = null;
    confirmSection.classList.add("hidden");
  }
}

// Action button handler - sends "retry" in failed state, "connect" otherwise
document.getElementById("actionBtn").addEventListener("click", () => {
  const btn = document.getElementById("actionBtn");
  const msgType = btn.textContent === "Retry" ? "retry" : "connect";
  chrome.runtime.sendMessage({ type: msgType });
});

// --- Legacy Token Handlers (Deprecated in v0.8.0) ---
// Note: Tokenless mode is now default. These handlers are kept for backward
// compatibility with older bridge versions but are no longer shown in UI.

function isValidTokenFormat(token) {
  return (
    typeof token === "string" &&
    token.startsWith("abk_") &&
    token.length === 36 &&
    /^[0-9a-f]+$/.test(token.slice(4))
  );
}

// Token save handler (legacy, tokenSection is hidden by default)
document.getElementById("tokenSaveBtn")?.addEventListener("click", () => {
  const token = document.getElementById("tokenInput").value.trim();
  if (!isValidTokenFormat(token)) {
    document.getElementById("tokenInput").style.borderColor = "#ef4444";
    return;
  }
  document.getElementById("tokenInput").style.borderColor = "";
  chrome.runtime.sendMessage({ type: "setToken", token });
  document.getElementById("tokenInput").value = "";
});

// Allow Enter key to save token (legacy)
document.getElementById("tokenInput")?.addEventListener("keydown", (e) => {
  if (e.key === "Enter") {
    document.getElementById("tokenSaveBtn").click();
  }
});

// L3 confirmation handlers - include nonce for request binding
document.getElementById("confirmAllow").addEventListener("click", () => {
  chrome.runtime.sendMessage({ type: "l3Response", allowed: true, nonce: currentL3Nonce });
  updateL3UI(null);
});

document.getElementById("confirmDeny").addEventListener("click", () => {
  chrome.runtime.sendMessage({ type: "l3Response", allowed: false, nonce: currentL3Nonce });
  updateL3UI(null);
});

// Check if token exists (legacy - kept for backward compatibility)
chrome.storage.local.get("bridgeToken", (result) => {
  const tokenInput = document.getElementById("tokenInput");
  if (tokenInput && result.bridgeToken) {
    tokenInput.placeholder = "Token saved (paste to replace)";
  }
});

// --- Cloud mode UI ---

const modeSelect = document.getElementById("modeSelect");
const deviceRow = document.getElementById("deviceRow");
const deviceLabel = document.getElementById("deviceLabel");
const cloudActionRow = document.getElementById("cloudActionRow");
const cloudSignOutRow = document.getElementById("cloudSignOutRow");
const cloudSignInBtn = document.getElementById("cloudSignInBtn");
const cloudSignOutBtn = document.getElementById("cloudSignOutBtn");

// Shown only when mode=cloud.
function renderCloudSection(mode, cloudToken, deviceId) {
  if (mode !== "cloud") {
    deviceRow.classList.add("hidden");
    cloudActionRow.classList.add("hidden");
    cloudSignOutRow.classList.add("hidden");
    return;
  }
  // Device row always shown in cloud mode (even if not signed in — shows "--")
  deviceRow.classList.remove("hidden");
  deviceLabel.textContent = deviceId
    ? deviceId.slice(0, 10) + (deviceId.length > 10 ? "…" : "")
    : "—";

  if (cloudToken) {
    cloudActionRow.classList.add("hidden");
    cloudSignOutRow.classList.remove("hidden");
  } else {
    cloudActionRow.classList.remove("hidden");
    cloudSignOutRow.classList.add("hidden");
  }
}

async function refreshCloudUi() {
  const { mode, cloudToken, deviceId } = await chrome.storage.local.get([
    "mode",
    "cloudToken",
    "deviceId",
  ]);
  const current = mode === "cloud" ? "cloud" : "local";
  modeSelect.value = current;
  renderCloudSection(current, cloudToken, deviceId);
}

modeSelect.addEventListener("change", () => {
  chrome.runtime.sendMessage({ type: "setMode", mode: modeSelect.value });
  // Re-render shortly so the UI reflects the new mode
  setTimeout(refreshCloudUi, 100);
});

cloudSignInBtn?.addEventListener("click", () => {
  // Default pair URL; override via chrome.storage.local.cloudPairUrl if needed.
  // OAuth module owns /pair, so this URL is part of the cross-module contract.
  chrome.storage.local.get(["cloudPairUrl"], ({ cloudPairUrl }) => {
    const base = cloudPairUrl || "https://edge.actionbook.dev/pair";
    const url = `${base}?extensionId=${encodeURIComponent(chrome.runtime.id)}`;
    chrome.tabs.create({ url });
  });
});

cloudSignOutBtn?.addEventListener("click", () => {
  chrome.runtime.sendMessage({ type: "cloud_sign_out" });
  setTimeout(refreshCloudUi, 100);
});

// Refresh when storage changes (e.g. callback.html stored a fresh token)
chrome.storage.onChanged.addListener((changes, area) => {
  if (area !== "local") return;
  if (changes.cloudToken || changes.mode || changes.deviceId) {
    refreshCloudUi();
  }
});

// Initial render
refreshCloudUi();

// Tab-grouping toggle — reflects and writes background's in-memory
// groupingEnabled flag; background persists to chrome.storage.local.
const groupTabsToggle = document.getElementById("groupTabsToggle");
chrome.runtime.sendMessage({ type: "getGrouping" }, (response) => {
  if (response && typeof response.enabled === "boolean") {
    groupTabsToggle.checked = response.enabled;
  }
});
groupTabsToggle.addEventListener("change", () => {
  chrome.runtime.sendMessage({
    type: "setGrouping",
    enabled: groupTabsToggle.checked,
  });
});

// Set version from manifest
document.getElementById("versionLabel").textContent =
  "v" + chrome.runtime.getManifest().version;

// Get initial state
chrome.runtime.sendMessage({ type: "getState" }, (response) => {
  if (response) updateUI(response);
});

// Get initial L3 status
chrome.runtime.sendMessage({ type: "getL3Status" }, (response) => {
  if (response) updateL3UI(response.pending);
});

// Listen for state updates and L3 notifications
chrome.runtime.onMessage.addListener((message) => {
  if (message.type === "stateUpdate") {
    updateUI(message);
  }
  if (message.type === "l3Status") {
    updateL3UI(message.pending);
  }
});

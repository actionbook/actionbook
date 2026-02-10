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
      bridgeDot.className = "dot red";
      bridgeStatus.textContent = "Token required";
      tokenSection.classList.remove("hidden");
      break;
    case "idle":
    default:
      bridgeDot.className = "dot gray";
      bridgeStatus.textContent = "Not connected";
      tokenSection.classList.remove("hidden");
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

// Token format: "abk_" prefix + 32 hex chars = 36 total
function isValidTokenFormat(token) {
  return (
    typeof token === "string" &&
    token.startsWith("abk_") &&
    token.length === 36 &&
    /^[0-9a-f]+$/.test(token.slice(4))
  );
}

// Token save handler
document.getElementById("tokenSaveBtn").addEventListener("click", () => {
  const token = document.getElementById("tokenInput").value.trim();
  if (!isValidTokenFormat(token)) {
    document.getElementById("tokenInput").style.borderColor = "#ef4444";
    return;
  }
  document.getElementById("tokenInput").style.borderColor = "";
  chrome.runtime.sendMessage({ type: "setToken", token });
  document.getElementById("tokenInput").value = "";
});

// Allow Enter key to save token
document.getElementById("tokenInput").addEventListener("keydown", (e) => {
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

// Check if token exists (show indicator, but never pre-populate the value)
chrome.storage.local.get("bridgeToken", (result) => {
  if (result.bridgeToken) {
    document.getElementById("tokenInput").placeholder = "Token saved (paste to replace)";
  }
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

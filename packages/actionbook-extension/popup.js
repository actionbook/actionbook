// Popup script - displays connection status and provides connect/retry/token/L3 confirmation controls

// Cached so renderCloudSection can re-trigger updateUI when mode/token change
// without waiting for the next stateUpdate from background.
let lastState = { connectionState: "idle", attachedTabIds: [], retryCount: 0, maxRetries: 0 };
let isCloudMode = false;
let hasCloudToken = false;

function updateUI(state) {
  lastState = state;
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
      bridgeStatus.textContent = "Ready";
      break;
    case "connecting":
      bridgeDot.className = "dot yellow";
      bridgeStatus.textContent = "Connecting…";
      if (state.retryCount > 0) {
        retryInfo.textContent = `Try ${state.retryCount}/${state.maxRetries}`;
      }
      break;
    case "disconnected":
      bridgeDot.className = "dot orange";
      bridgeStatus.textContent = "Reconnecting…";
      if (state.retryCount > 0) {
        retryInfo.textContent = `Try ${state.retryCount}/${state.maxRetries}`;
      }
      actionBtn.textContent = "Connect";
      actionBtn.classList.remove("hidden");
      break;
    case "failed":
      bridgeDot.className = "dot red";
      bridgeStatus.textContent = "Unable to connect";
      retryInfo.textContent = "";
      actionBtn.textContent = "Try again";
      actionBtn.classList.remove("hidden");
      break;
    case "pairing_required":
      // Deprecated: Token no longer required, treat as disconnected
      bridgeDot.className = "dot orange";
      bridgeStatus.textContent = "Waiting to connect";
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

  const attachedTabs = Array.isArray(state.attachedTabIds) ? state.attachedTabIds : [];
  if (attachedTabs.length > 0) {
    tabDot.className = "dot green";
    tabStatus.textContent = attachedTabs.length === 1
      ? "Working on a tab"
      : `Working on ${attachedTabs.length} tabs`;
  } else {
    tabDot.className = "dot gray";
    tabStatus.textContent = "Idle";
  }

  // In cloud mode, Connect/Try-again without a token is a no-op — background
  // bails out at config load. Hide it so the user only sees "Sign in".
  if (isCloudMode && !hasCloudToken) {
    actionBtn.classList.add("hidden");
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
  const msgType = btn.textContent === "Try again" ? "retry" : "connect";
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
    tokenInput.placeholder = "Already saved — paste a new one to replace";
  }
});

// --- Cloud mode UI ---

const modeSelect = document.getElementById("modeSelect");
const modeTrigger = modeSelect.querySelector(".mode-select__trigger");
const modeLabel = document.getElementById("modeSelectLabel");
const modeMenu = modeSelect.querySelector(".mode-select__menu");
const modeOptions = modeMenu.querySelectorAll(".mode-select__option");
const cloudActionRow = document.getElementById("cloudActionRow");
const cloudSignOutRow = document.getElementById("cloudSignOutRow");
const cloudSignInBtn = document.getElementById("cloudSignInBtn");
const cloudSignOutBtn = document.getElementById("cloudSignOutBtn");
const localHint = document.getElementById("localHint");
const cloudHint = document.getElementById("cloudHint");

// --- Custom Mode dropdown ---
function setModeValue(value) {
  modeSelect.dataset.value = value;
  modeOptions.forEach((opt) => {
    const isSelected = opt.dataset.value === value;
    opt.setAttribute("aria-selected", isSelected ? "true" : "false");
    if (isSelected) {
      modeLabel.textContent = opt.querySelector(".mode-select__option-header span").textContent;
    }
  });
}

function openModeMenu() {
  modeSelect.dataset.open = "true";
  modeTrigger.setAttribute("aria-expanded", "true");
}

function closeModeMenu() {
  modeSelect.dataset.open = "false";
  modeTrigger.setAttribute("aria-expanded", "false");
}

modeTrigger.addEventListener("click", (e) => {
  e.stopPropagation();
  if (modeSelect.dataset.open === "true") closeModeMenu();
  else openModeMenu();
});

modeOptions.forEach((opt) => {
  opt.addEventListener("click", (e) => {
    e.stopPropagation();
    const newValue = opt.dataset.value;
    if (newValue !== modeSelect.dataset.value) {
      setModeValue(newValue);
      chrome.runtime.sendMessage({ type: "setMode", mode: newValue });
      setTimeout(refreshCloudUi, 100);
    }
    closeModeMenu();
  });
});

document.addEventListener("click", (e) => {
  if (!modeSelect.contains(e.target)) closeModeMenu();
});

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && modeSelect.dataset.open === "true") {
    closeModeMenu();
    modeTrigger.focus();
  }
});

// Shown only when mode=cloud.
function renderCloudSection(mode, cloudToken) {
  // Footer hint is mode-specific.
  if (localHint) localHint.classList.toggle("hidden", mode !== "local");
  if (cloudHint) cloudHint.classList.toggle("hidden", mode !== "cloud");

  if (mode !== "cloud") {
    cloudActionRow.classList.add("hidden");
    cloudSignOutRow.classList.add("hidden");
    return;
  }

  if (cloudToken) {
    cloudActionRow.classList.add("hidden");
    cloudSignOutRow.classList.remove("hidden");
  } else {
    cloudActionRow.classList.remove("hidden");
    cloudSignOutRow.classList.add("hidden");
  }
}

async function refreshCloudUi() {
  const { mode, cloudToken } = await chrome.storage.local.get([
    "mode",
    "cloudToken",
  ]);
  const current = mode === "local" ? "local" : "cloud";
  isCloudMode = current === "cloud";
  hasCloudToken = !!cloudToken;
  setModeValue(current);
  renderCloudSection(current, cloudToken);
  // Re-render the action button against the latest mode/token — otherwise the
  // Connect button would linger visible between the stateUpdate and the next.
  updateUI(lastState);
}

// Sign in starts the OAuth 2.1 authorization-code + PKCE flow against Clerk.
// We generate a PKCE verifier here, stash it in chrome.storage.local keyed by
// `state`, and open Clerk's authorize URL in a new tab. When Clerk redirects
// back to chrome-extension://<id>/callback.html?code=...&state=..., callback.js
// reads the verifier by state, exchanges the code for an access token, and
// signals background.js to reconnect.
cloudSignInBtn?.addEventListener("click", async () => {
  const cfg = self.ACTIONBOOK_CLOUD_CONFIG;

  // Sanity check: warn if dev-build id doesn't match Clerk's whitelisted URI.
  // Prod / CWS build will have a different id — add it to cloud-config.js then.
  if (!cfg.EXPECTED_EXTENSION_IDS.includes(chrome.runtime.id)) {
    console.warn(
      "[actionbook] unexpected extension id:",
      chrome.runtime.id,
      "— Clerk's redirect-URI whitelist may not include this build, sign-in will fail."
    );
  }

  const { verifier, challenge } = await generatePkcePair();
  const state = crypto.randomUUID();

  // Stash by state so callback.js can retrieve the exact verifier used here.
  // Short TTL is enforced implicitly by the state check — callback.js deletes
  // the entry after using it, and we don't trust stale entries.
  await chrome.storage.local.set({
    [`pkce:${state}`]: { verifier, createdAt: Date.now() },
  });

  const redirectUri = `chrome-extension://${chrome.runtime.id}/callback.html`;
  const params = new URLSearchParams({
    client_id: cfg.CLERK_CLIENT_ID,
    response_type: "code",
    redirect_uri: redirectUri,
    scope: cfg.CLERK_SCOPES,
    state,
    code_challenge: challenge,
    code_challenge_method: "S256",
  });

  chrome.tabs.create({ url: `${cfg.CLERK_AUTHORIZE_URL}?${params.toString()}` });
});

// PKCE helpers: verifier is a 43–128 char random URL-safe string; challenge is
// base64url(SHA256(verifier)) for S256 flow. RFC 7636.
async function generatePkcePair() {
  const verifierBytes = new Uint8Array(32);
  crypto.getRandomValues(verifierBytes);
  const verifier = base64Url(verifierBytes);
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier));
  const challenge = base64Url(new Uint8Array(digest));
  return { verifier, challenge };
}

function base64Url(bytes) {
  let s = "";
  for (let i = 0; i < bytes.length; i++) s += String.fromCharCode(bytes[i]);
  return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

cloudSignOutBtn?.addEventListener("click", () => {
  chrome.runtime.sendMessage({ type: "cloud_sign_out" });
  setTimeout(refreshCloudUi, 100);
});

// Refresh when storage changes (e.g. callback.html stored a fresh token)
chrome.storage.onChanged.addListener((changes, area) => {
  if (area !== "local") return;
  if (changes.cloudToken || changes.mode) {
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

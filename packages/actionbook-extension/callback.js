// Invoked when the OAuth module redirects back to
//   chrome-extension://<EXTENSION_ID>/callback.html?token=<token>[&deviceId=<id>]
// or on failure:
//   chrome-extension://<EXTENSION_ID>/callback.html?error=<code>[&error_description=<msg>]
//
// We read the query params, persist the token to chrome.storage.local,
// notify background.js so it reconnects with the new token, then close.

const msgEl = document.getElementById("msg");
const detailEl = document.getElementById("detail");

function showError(code, detail) {
  msgEl.textContent = `Sign-in failed: ${code}`;
  msgEl.className = "error";
  if (detail) detailEl.textContent = detail;
}

function showSuccess() {
  msgEl.textContent = "Signed in. You can close this tab.";
}

(async () => {
  const params = new URLSearchParams(location.search);
  const token = params.get("token");
  const deviceIdParam = params.get("deviceId");
  const error = params.get("error");
  const errorDescription = params.get("error_description");

  if (error) {
    showError(error, errorDescription);
    return;
  }

  if (!token) {
    showError("missing_token", "The redirect did not include a token parameter.");
    return;
  }

  // Persist and signal background.
  const update = { cloudToken: token, mode: "cloud" };
  if (deviceIdParam) update.deviceId = deviceIdParam;

  try {
    await chrome.storage.local.set(update);
    await chrome.runtime.sendMessage({ type: "cloud_auth_updated" });
  } catch (err) {
    showError("storage_failed", err?.message || String(err));
    return;
  }

  showSuccess();
  setTimeout(() => {
    try { window.close(); } catch (_) {}
  }, 1500);
})();

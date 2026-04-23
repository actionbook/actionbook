// Invoked when Clerk redirects back to
//   chrome-extension://<EXTENSION_ID>/callback.html?code=<code>&state=<state>
// (success) or
//   chrome-extension://<EXTENSION_ID>/callback.html?error=<code>[&error_description=<msg>]
// (failure).
//
// Flow:
//   1. Pull ?code + ?state from URL
//   2. Look up the PKCE verifier we stashed under pkce:<state> when kicking off
//      the authorize request in popup.js
//   3. POST to Clerk's /oauth/token with code + code_verifier → access_token
//      (+ refresh_token if offline_access scope was granted)
//   4. Persist to chrome.storage.local and tell background.js to reconnect

const msgEl = document.getElementById("msg");
const detailEl = document.getElementById("detail");

// Map internal error codes to user-friendly messages. Unknown codes fall back
// to a generic phrase; the `detail` line (often from the OAuth server) still
// gets surfaced below in case the user needs to share it with support.
const ERROR_MESSAGES = {
  missing_params: "The sign-in link was incomplete. Please try again.",
  pkce_missing: "This sign-in link has expired. Please start again from the extension.",
  network: "Couldn’t reach the sign-in server. Check your internet connection and try again.",
  parse_failed: "The sign-in server sent an unexpected response. Please try again.",
  no_access_token: "Sign-in didn’t complete. Please try again.",
  storage_failed: "Couldn’t save your sign-in. Please try again.",
  access_denied: "Sign-in was cancelled.",
};

function friendlyMessage(code) {
  if (ERROR_MESSAGES[code]) return ERROR_MESSAGES[code];
  if (typeof code === "string" && code.startsWith("token_")) {
    return "The sign-in server rejected the request. Please try again.";
  }
  return "Sign-in didn’t complete. Please try again.";
}

function showError(code, detail) {
  msgEl.textContent = friendlyMessage(code);
  msgEl.className = "error";
  if (detail) detailEl.textContent = detail;
}

function showSuccess() {
  msgEl.textContent = "You’re signed in. You can close this tab.";
}

(async () => {
  const cfg = self.ACTIONBOOK_CLOUD_CONFIG;

  const params = new URLSearchParams(location.search);
  const code = params.get("code");
  const state = params.get("state");
  const error = params.get("error");
  const errorDescription = params.get("error_description");

  if (error) {
    showError(error, errorDescription);
    return;
  }

  if (!code || !state) {
    showError("missing_params", "The redirect URL didn't include code + state.");
    return;
  }

  // Retrieve + immediately remove the one-shot PKCE verifier stashed by popup.js.
  const stashKey = `pkce:${state}`;
  const stash = (await chrome.storage.local.get(stashKey))[stashKey];
  if (!stash || !stash.verifier) {
    showError(
      "pkce_missing",
      "No PKCE verifier found for this state — did you start sign-in from a different session?"
    );
    return;
  }
  await chrome.storage.local.remove(stashKey);

  // Exchange code for access token (+ refresh token if scope included offline_access).
  // Public client: no client_secret, PKCE verifier authenticates us instead.
  const redirectUri = `chrome-extension://${chrome.runtime.id}/callback.html`;
  let tokenRes;
  try {
    tokenRes = await fetch(cfg.CLERK_TOKEN_URL, {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({
        grant_type: "authorization_code",
        code,
        redirect_uri: redirectUri,
        client_id: cfg.CLERK_CLIENT_ID,
        code_verifier: stash.verifier,
      }),
    });
  } catch (err) {
    showError("network", err?.message || String(err));
    return;
  }

  if (!tokenRes.ok) {
    const body = await tokenRes.text();
    showError(`token_${tokenRes.status}`, body);
    return;
  }

  let tokens;
  try {
    tokens = await tokenRes.json();
  } catch (err) {
    showError("parse_failed", err?.message || String(err));
    return;
  }

  if (!tokens.access_token) {
    showError("no_access_token", "Clerk did not return an access_token.");
    return;
  }

  // Persist + tell background to reconnect. refreshToken is optional — only
  // present if the user granted offline_access scope (they will have, per
  // cloud-config.js, but be tolerant if Clerk omits it).
  const update = { cloudToken: tokens.access_token, mode: "cloud" };
  if (typeof tokens.refresh_token === "string") {
    update.cloudRefreshToken = tokens.refresh_token;
  }
  if (typeof tokens.expires_in === "number") {
    update.cloudTokenExpiresAt = Date.now() + tokens.expires_in * 1000;
  }

  try {
    await chrome.storage.local.set(update);
    await chrome.runtime.sendMessage({ type: "cloud_auth_updated" });
  } catch (err) {
    showError("storage_failed", err?.message || String(err));
    return;
  }

  showSuccess();
  setTimeout(() => {
    try {
      window.close();
    } catch (_) {}
  }, 1500);
})();

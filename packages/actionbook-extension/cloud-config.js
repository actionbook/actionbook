// Cloud-mode OAuth configuration. These values are public — a Chrome extension
// is a "public" OAuth client and cannot hold secrets. Token exchange relies on
// PKCE (code_verifier) instead of a client_secret, so the client_id being
// visible here is by design and matches what is registered in Clerk Dashboard.

// The "Actionbook Extension" OAuth application registered in Clerk.
const CLERK_CLIENT_ID = "HP91Xj6adCm3TjPr";

// Clerk authorization server endpoints (all public, see clerk.actionbook.dev
// /.well-known/oauth-authorization-server).
const CLERK_AUTHORIZE_URL = "https://clerk.actionbook.dev/oauth/authorize";
const CLERK_TOKEN_URL = "https://clerk.actionbook.dev/oauth/token";

// What we ask Clerk for on the user's behalf.
// - openid: required for `sub` claim in the JWT (= userId for DO routing)
// - profile/email: surfaced in popup + audit logs
// - offline_access: issues a refresh_token so we can rotate expired access tokens
//   without dragging the user back through the sign-in flow
const CLERK_SCOPES = "openid profile email offline_access";

// Stable dev-build extension ID derived from manifest.json `key` field. Used
// only as a sanity check: if chrome.runtime.id differs we warn that the
// Clerk redirect-URI whitelist probably doesn't include the current build.
// The production (Chrome Web Store) build will have a different id — once we
// publish, add that id to Clerk's whitelist AND extend EXPECTED_EXTENSION_IDS.
const EXPECTED_EXTENSION_IDS = [
  "dpfioflkmnkklgjldmaggkodhlidkdcd", // local unpacked / dev
];

// Export for both importScripts (service worker) and plain <script> (popup / callback)
if (typeof self !== "undefined") {
  self.ACTIONBOOK_CLOUD_CONFIG = {
    CLERK_CLIENT_ID,
    CLERK_AUTHORIZE_URL,
    CLERK_TOKEN_URL,
    CLERK_SCOPES,
    EXPECTED_EXTENSION_IDS,
  };
}

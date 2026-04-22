# Privacy Policy

**Actionbook Browser Extension**
**Last Updated: April 2026**

## 1. Introduction

This Privacy Policy describes how the Actionbook browser extension ("the Extension"), developed by Actionbook ("we", "us", or "our"), handles information when you install and use the Extension. We are committed to protecting your privacy and being transparent about our data practices.

The Extension serves as a bridge between AI-powered automation agents and your browser. Starting with v0.5.0 it supports two independent modes:

- **Local Mode** (default): the Extension talks only to the Actionbook command-line interface (CLI) on `localhost`. Nothing leaves your machine.
- **Cloud Mode** (opt-in): the Extension authenticates you against Actionbook's sign-in provider (Clerk) and maintains an outbound WebSocket to `edge.actionbook.dev` so remote AI agents can drive your browser over MCP.

Sections 2A and 2B describe the two modes in detail. Section 3 lists what we **never** do regardless of mode.

## 2. Information We Access

### 2A. Local Mode (default)

All data access in Local Mode occurs entirely on your local machine and is never transmitted to any external server. The Extension communicates exclusively with `localhost` (127.0.0.1) via WebSocket.

#### 2A.1 Tab Information

The Extension accesses browser tab metadata (tab ID, title, URL, active state, window ID) to allow the local CLI to identify, attach to, and interact with browser tabs. This information is used only within the local WebSocket connection between the Extension and the CLI running on your machine.

#### 2A.2 Page Content via Chrome DevTools Protocol (CDP)

When a tab is attached for automation, the Extension uses the Chrome Debugger API to execute CDP commands on the attached tab. This may include reading DOM structure, capturing screenshots, evaluating JavaScript, dispatching input events, and navigating pages. All CDP commands are filtered through a strict allowlist and tiered security model (see Section 5).

#### 2A.3 Cookies

The Extension can read, set, and remove browser cookies for specific URLs when instructed by the local CLI. Cookie operations on sensitive domains (banking, payment, government, healthcare) require explicit user confirmation through the Extension popup before execution.

#### 2A.4 Bridge Token

The Extension stores a short-lived authentication token ("bridge token") in `chrome.storage.local`. This token is generated locally by the Actionbook CLI and is used solely to authenticate the WebSocket connection between the Extension and the CLI on localhost. The token follows the format `abk_` followed by 32 hexadecimal characters.

#### 2A.5 Bridge Port Configuration

The Extension stores the WebSocket port number (default: 19222) in `chrome.storage.local` to connect to the local CLI bridge server.

### 2B. Cloud Mode (opt-in, v0.5.0+)

Cloud Mode is **opt-in and reversible**. You enable it by selecting "Cloud" in the popup; you can return to Local Mode at any time. When Cloud Mode is enabled, the Extension transmits a narrow set of data to Actionbook-operated and Clerk-operated infrastructure, strictly in the service of routing your chosen AI agents' browser automation commands to your browser.

#### 2B.1 Authentication via Clerk

Sign-in for Cloud Mode uses [Clerk](https://clerk.com) (at `clerk.actionbook.dev`) through a standard OAuth 2.1 Authorization Code flow with PKCE. During sign-in, Clerk receives and processes your authentication credentials (email and, depending on your sign-in method, password or third-party provider). **The Extension itself never sees your password** — authentication happens entirely in Clerk's hosted sign-in page.

After sign-in, Clerk returns a short-lived **access token** (a signed JWT) and optionally a **refresh token**. The Extension stores these in `chrome.storage.local`; they are used only to authenticate subsequent connections to `edge.actionbook.dev`.

#### 2B.2 Device Identifier

On first Cloud Mode sign-in, the Extension generates a random per-install device ID (`d_` + UUID) and stores it in `chrome.storage.local`. The device ID is reported to the edge server during WebSocket handshake to label your connection (useful if you later use the Extension across multiple machines). It does not identify you personally.

#### 2B.3 Outbound WebSocket to edge.actionbook.dev

In Cloud Mode the Extension maintains a WebSocket connection to `wss://edge.actionbook.dev/extension/ws`. Over this connection flow the same categories of data described in Sections 2A.1–2A.3 (tab metadata, CDP commands and their results, cookie operations), **but only when an authenticated AI agent explicitly invokes a tool that requires them**. The edge server forwards those commands to your Extension and relays the responses back to the requesting agent.

Commands are processed ephemerally: the edge server does not persist CDP command content beyond the lifetime of the in-flight request-response cycle.

#### 2B.4 Authorized AI Agents

Agents that can send commands to your browser must present a valid Clerk-issued access token whose subject matches your user ID. In practice this means only agents you have explicitly authorized (e.g., by adding an MCP Connector in Claude Desktop and completing its OAuth prompt) can drive your browser. You can revoke any agent's session from your account settings on actionbook.dev or through Clerk's session management.

#### 2B.5 Token Refresh and Expiry

Access tokens expire quickly (minutes to an hour depending on Clerk configuration). The Extension uses the refresh token, if available, to transparently obtain new access tokens without re-prompting you. If refresh fails or the session has been revoked, the Extension returns to the "sign-in required" state and disconnects until you sign in again.

## 3. Information We Do NOT Collect

Regardless of mode, the Extension does **not**:

- **Collect, aggregate, or sell personal data.** We do not maintain analytics, aggregate browsing history, or sell user information to third parties.
- **Include analytics, telemetry, or crash reporting** of any kind.
- **Include advertising or monetization mechanisms.**
- **Read or transmit page content** beyond what AI agents explicitly request through tools you have authorized.

**Mode-specific clarifications:**

- In **Local Mode**, no data accessed by the Extension is ever sent to any external server. The WebSocket connection is restricted to `localhost` (127.0.0.1).
- In **Cloud Mode**, the only external destinations are `clerk.actionbook.dev` (during sign-in) and `edge.actionbook.dev` (during active operation). Data flows to either destination are listed exhaustively in Section 2B.

## 4. Data Storage and Retention

### 4.1 Local Storage Only

All data stored by the Extension resides in `chrome.storage.local` on your device. The items stored are:

| Data Item                | Mode  | Purpose                                                | Retention                                          |
|--------------------------|-------|--------------------------------------------------------|----------------------------------------------------|
| `bridgeToken`            | Local | Authenticate local WebSocket connection                | Cleared on disconnect or expiry                    |
| `bridgePort`             | Local | WebSocket port for local CLI bridge                    | Persists until changed                             |
| `mode`                   | Both  | User's selected mode (`"local"` or `"cloud"`)          | Persists until changed                             |
| `cloudToken`             | Cloud | OAuth access token (short-lived Clerk JWT)             | Cleared on sign-out or on-demand refresh rotation |
| `cloudRefreshToken`      | Cloud | OAuth refresh token (used to rotate access tokens)     | Cleared on sign-out                                |
| `cloudTokenExpiresAt`    | Cloud | Expected access-token expiry timestamp                 | Cleared on sign-out                                |
| `deviceId`               | Cloud | Per-install device identifier                          | Persists across sign-outs to stabilize device naming |
| `cloudEndpoint`          | Cloud | Edge server WebSocket URL (default `wss://edge.actionbook.dev/extension/ws`) | Persists; only changes if you override it        |
| `pkce:<state>`           | Cloud | One-shot PKCE verifier, written during sign-in         | Deleted immediately after token exchange           |

### 4.2 Token Expiration

- **Local Mode bridge tokens** expire automatically after 30 minutes of inactivity, or when the WebSocket connection, handshake, or server-side validity check fails.
- **Cloud Mode access tokens** are short-lived (typically minutes to one hour). The Extension refreshes them automatically using the refresh token; if refresh fails, the Extension clears the access token and returns to a signed-out state.

### 4.3 No Persistent Data

The Extension does not maintain any persistent logs, history, or caches of browser content, page data, or automation activity. All operational state exists only in memory during the active service worker session and is discarded when the service worker terminates.

## 5. Security Model

The Extension implements a three-tier security gating model for all browser commands:

### Level 1 (L1) - Read Only

Commands that only read data (e.g., capturing screenshots, querying DOM structure, reading cookies) are auto-approved. These operations do not modify any browser state.

### Level 2 (L2) - Page Modification

Commands that modify page state (e.g., navigating, clicking, typing, evaluating JavaScript) are auto-approved with internal logging. On sensitive domains (banking, payment, government, healthcare), L2 commands are automatically elevated to L3.

### Level 3 (L3) - High Risk

Commands that perform high-risk operations (e.g., setting cookies, deleting cookies, clearing site data) require explicit user confirmation through the Extension popup before execution. The user must click "Allow" or "Deny" within 30 seconds, after which the command times out and is denied. Only one L3 confirmation can be pending at a time.

### Additional Security Measures

- **CDP Command Allowlist**: Only a curated set of Chrome DevTools Protocol methods are permitted. Any method not on the allowlist is rejected.
- **Sensitive Domain Detection**: Domains matching patterns for banking, payment, government, and healthcare sites trigger elevated security requirements.
- **Token Format Validation**: All tokens are validated against strict formats before acceptance.
- **Popup-Only Configuration Changes**: Mode switches and token updates are accepted only from the Extension's own popup or sign-in callback page, verified by sender identity.
- **Localhost-Only Communication in Local Mode**: The WebSocket connection is restricted to `localhost` (127.0.0.1).
- **HTTPS/WSS-Only External Communication in Cloud Mode**: All external traffic uses TLS (WSS for the WebSocket, HTTPS for OAuth endpoints).

## 6. Permissions Justification

The Extension requests the following Chrome permissions, each necessary for its core functionality:

| Permission        | Why It Is Needed                                                      |
|-------------------|-----------------------------------------------------------------------|
| `debugger`        | Attach Chrome DevTools Protocol to tabs for browser automation         |
| `tabs`            | List and query browser tabs so AI agents can select automation targets |
| `tabGroups`       | Group tabs opened by the Extension into a dedicated Chrome tab group   |
| `activeTab`       | Access the currently active tab for automation commands                 |
| `offscreen`       | Keep the service worker alive for persistent WebSocket connection      |
| `storage`         | Store local connection state (mode, tokens, device ID)                 |
| `cookies`         | Read and manage cookies for web automation tasks                       |
| `<all_urls>`      | Enable automation on any website the user chooses to automate          |

## 7. Changes to This Policy

We may update this Privacy Policy from time to time. Any changes will be reflected in the "Last Updated" date at the top of this document. We encourage you to review this policy periodically.

## 8. Contact Us

If you have questions or concerns about this Privacy Policy or the Extension's data practices, please contact us at:

**Email**: [contact@actionbook.dev]

## 9. Open Source

The Actionbook Extension source code is available for inspection. Users and security researchers are welcome to review the Extension's code to verify the data practices described in this policy.

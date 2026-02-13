# Privacy Policy

**Actionbook Browser Extension**
**Last Updated: February 2026**

## 1. Introduction

This Privacy Policy describes how the Actionbook browser extension ("the Extension"), developed by Actionbook ("we", "us", or "our"), handles information when you install and use the Extension. We are committed to protecting your privacy and being transparent about our data practices.

The Extension serves as a local bridge between the Actionbook command-line interface (CLI) and your browser, enabling AI-powered browser automation on your machine.

## 2. Information We Access

The Extension accesses certain browser data solely to perform its core function of local browser automation. All data access occurs entirely on your local machine and is never transmitted to any external server.

### 2.1 Tab Information

The Extension accesses browser tab metadata (tab ID, title, URL, active state, window ID) to allow the local CLI to identify, attach to, and interact with browser tabs. This information is used only within the local WebSocket connection between the Extension and the CLI running on your machine.

### 2.2 Page Content via Chrome DevTools Protocol (CDP)

When a tab is attached for automation, the Extension uses the Chrome Debugger API to execute CDP commands on the attached tab. This may include reading DOM structure, capturing screenshots, evaluating JavaScript, dispatching input events, and navigating pages. All CDP commands are filtered through a strict allowlist and tiered security model (see Section 5).

### 2.3 Cookies

The Extension can read, set, and remove browser cookies for specific URLs when instructed by the local CLI. Cookie operations on sensitive domains (banking, payment, government, healthcare) require explicit user confirmation through the Extension popup before execution.

### 2.4 Bridge Token

The Extension stores a short-lived authentication token ("bridge token") in `chrome.storage.local`. This token is generated locally by the Actionbook CLI and is used solely to authenticate the WebSocket connection between the Extension and the CLI on localhost. The token follows the format `abk_` followed by 32 hexadecimal characters.

### 2.5 Bridge Port Configuration

The Extension stores the WebSocket port number (default: 19222) in `chrome.storage.local` to connect to the local CLI bridge server.

## 3. Information We Do NOT Collect

We want to be explicit about what we do not do:

- **No data collection.** We do not collect, record, or aggregate any personal information, browsing history, page content, cookies, or any other user data.
- **No external transmission.** No data accessed by the Extension is ever sent to any external server, cloud service, or third-party endpoint. The Extension communicates exclusively with `localhost` (127.0.0.1) via WebSocket.
- **No analytics or tracking.** The Extension contains no analytics, telemetry, crash reporting, or usage tracking of any kind.
- **No user accounts.** The Extension does not require or support user accounts, sign-ins, or registration.
- **No advertising.** The Extension contains no advertising, ad networks, or monetization mechanisms.
- **No third-party sharing.** No data is shared with, sold to, or made available to any third party.

## 4. Data Storage and Retention

### 4.1 Local Storage Only

All data stored by the Extension resides in `chrome.storage.local` on your device. The only items stored are:

| Data Item     | Purpose                                      | Retention                     |
|---------------|----------------------------------------------|-------------------------------|
| `bridgeToken` | Authenticate local WebSocket connection       | Cleared on disconnect or expiry |
| `bridgePort`  | WebSocket port for local CLI bridge           | Persists until changed         |

### 4.2 Token Expiration

Bridge tokens expire automatically after 30 minutes of inactivity. When a token expires, the Extension clears it from local storage and requires re-authentication with the CLI. The Extension also clears the stored token when:

- The WebSocket connection is lost
- The server reports the token as expired or invalid
- A handshake with the bridge server fails

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
- **Token Validation**: All tokens are validated against a strict format before acceptance.
- **Popup-Only Token Input**: Token changes are accepted only from the Extension's own popup, verified by sender identity.
- **Localhost-Only Communication**: The WebSocket connection is restricted to `localhost` (127.0.0.1).

## 6. Permissions Justification

The Extension requests the following Chrome permissions, each necessary for its core functionality:

| Permission        | Why It Is Needed                                                      |
|-------------------|-----------------------------------------------------------------------|
| `debugger`        | Attach Chrome DevTools Protocol to tabs for browser automation         |
| `tabs`            | List and query browser tabs for the local CLI to select targets        |
| `activeTab`       | Access the currently active tab for automation commands                 |
| `scripting`       | Execute scripts in the context of web pages for automation             |
| `offscreen`       | Keep the service worker alive for persistent WebSocket connection      |
| `storage`         | Store local connection state for bridge communication                   |
| `cookies`         | Read and manage cookies for web automation tasks                       |
| `<all_urls>`      | Enable automation on any website the user chooses to automate          |

## 7. Changes to This Policy

We may update this Privacy Policy from time to time. Any changes will be reflected in the "Last Updated" date at the top of this document. We encourage you to review this policy periodically.

## 8. Contact Us

If you have questions or concerns about this Privacy Policy or the Extension's data practices, please contact us at:

**Email**: [contact@actionbook.dev]

## 9. Open Source

The Actionbook Extension source code is available for inspection. Users and security researchers are welcome to review the Extension's code to verify the data practices described in this policy.

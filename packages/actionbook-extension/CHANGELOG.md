# @actionbookdev/extension

## 0.5.0

### Minor Changes

- Add Cloud Mode: connect the Extension to Actionbook's edge server so remote AI agents can drive your Chrome over MCP without running the local CLI daemon.

  - New popup toggle switches between **Local (CLI)** mode — identical to previous versions — and **Cloud** mode.
  - In Cloud Mode, the Extension authenticates the user against `clerk.actionbook.dev` via OAuth 2.1 Authorization Code + PKCE, then maintains a WebSocket to `wss://edge.actionbook.dev/extension/ws`.
  - Any MCP client pointed at `https://edge.actionbook.dev/mcp` (e.g., Claude Desktop, claude.ai Connectors, Codex) can now drive the user's Chrome, authenticated by the same user identity.
  - Access tokens rotate automatically via refresh token; expired tokens trigger a silent refresh instead of forcing re-sign-in.
  - Sign out from the popup clears all cloud tokens and disconnects.
  - **Local Mode is unchanged and remains the default.** Existing Local Mode users upgrading from 0.4.x see no behavior change; no new permissions are requested.

- Bump bridge protocol to `0.5.0` in the `hello` frame. CLI `EXTENSION_PROTOCOL_MIN_VERSION` remains `0.4.0`, so the new Extension is backward-compatible with older CLIs.

- New files: `cloud-config.js` (public OAuth client id + Clerk endpoints), `callback.html` / `callback.js` (OAuth redirect handler). `callback.html` is listed in `web_accessible_resources` to allow the sign-in page to redirect back into the Extension.

### Documentation

- README.md — adds a "Cloud Mode" section with setup, Claude Desktop config example, and mode-switching instructions.
- PRIVACY.md — substantially revised to describe the two operating modes separately. Cloud Mode data flows (Clerk authentication, device identifier, outbound WebSocket to `edge.actionbook.dev`, authorized agents, token refresh) are enumerated in a new Section 2B. Local Mode wording is unchanged.

## 0.4.2

### Patch Changes

- [#570](https://github.com/actionbook/actionbook/pull/570) [`78c7840`](https://github.com/actionbook/actionbook/commit/78c78401557f2b4e5dd8e97f27429ed0e17f06cf) Thanks [@mcfn](https://github.com/mcfn)! - Prefix the Actionbook Chrome tab-group title with the bowtie logo ⋈ so agent-driven tabs are identifiable at a glance (ACT-994). Existing installs will create a new "⋈ Actionbook" group on upgrade; any previously-named "Actionbook" group becomes orphaned and can be closed manually.

## 0.4.1

### Patch Changes

- [`12669c9`](https://github.com/actionbook/actionbook/commit/12669c9aaf696733453ee668d5322ac4b5a60ae3) Thanks [@mcfn](https://github.com/mcfn)! - Simplify tab ownership model: `Extension.listTabs` now returns only tabs in the Actionbook group (drag in = appears, drag out = disappears). `Extension.attachTab` always moves the tab into the group. Remove the unused `ACTIONBOOK_GROUP_ATTACH` flag.

## 0.4.0

### Minor Changes

- Group Actionbook-opened tabs into a dedicated Chrome tab group.

  - Tabs opened via `Extension.createTab` (including the reuse-empty-tab path) are automatically moved into a per-window tab group titled "Actionbook" (blue). Makes it easy to tell agent-driven tabs apart from your own and bulk-collapse/close them.
  - Adds the `tabGroups` permission to the extension manifest.
  - New popup toggle "Group Actionbook tabs" (default on); preference persists in `chrome.storage.local` under `groupTabs`.
  - User-attached existing tabs (`Extension.attachTab`) are **not** moved by default — controlled by the internal `ACTIONBOOK_GROUP_ATTACH` flag to preserve user intent.

- Scope `Extension.listTabs` to Actionbook-managed tabs only (protocol 0.4.0).

  - `Extension.listTabs` now returns only tabs attached or created via Actionbook, instead of every tab in the browser. Prevents agents from accidentally operating on the user's unrelated tabs.
  - Bumps the extension bridge protocol to `0.4.0`.

- Enable the CDP `Network` domain in extension mode so HAR captures traffic.

  - In extension-bridge mode, the `Network` domain is now enabled on attach (and re-enabled after self-heal reattach), so `har start` / `har stop` produce non-empty HAR output when driving a Chrome tab via the extension.

## 0.3.0

### Minor Changes

- [#533](https://github.com/actionbook/actionbook/pull/533) [`e429866`](https://github.com/actionbook/actionbook/commit/e429866115d75475eaafaa91cdfcbaa489d95df2) Thanks [@mcfn](https://github.com/mcfn)! - Release 0.3.0: align extension bridge with Actionbook CLI 1.x.

  - Support CLI 1.x stateless architecture — every message is self-contained with explicit `--session`/`--tab` addressing, no implicit current-tab state.
  - Concurrent multi-tab operation: bridge protocol upgraded to handle parallel CDP traffic across multiple tabs in a single session.
  - Health check on startup to prevent connect/disconnect loops.

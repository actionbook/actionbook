# @actionbookdev/extension

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

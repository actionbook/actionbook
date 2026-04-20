# @actionbookdev/cli

## 1.5.1

### Patch Changes

- - Fix duplicate short tab IDs after drag-out-drag-in in extension mode: `push_tab` skips already-occupied candidates;
  - Fix plain-text error output routed to stderr instead of stdout
  - Fix case-insensitive internal-scheme detection
  - Fix network-idle check incorrectly blocking on off-screen lazy images
  - Fix wait stability with dual-threshold for already-loaded baseline URL

## 1.5.0

### Minor Changes

- Release 1.5.0: network HAR recording, search/manual commands, and extension-mode enhancement.

  - **Network HAR recording**: new `browser network har start` / `har stop` commands capture HTTP traffic to a HAR file. Works in both local and extension bridge modes; isolated per session.
  - **Relaxed network-idle mode**: new `wait --relaxed-idle` for pages with persistent background traffic (websockets, long-poll); considers the page idle while at most N requests are in flight.
  - **`search` / `manual` top-level commands**: search Actionbook actions and fetch action manuals directly from the CLI; both now listed in top-level help.
  - **Session IDs may contain underscores**.
  - **Extension-mode**:
    - Scope all tab operations to the Actionbook-designated chrome.tabGroups group; tabs outside this group are ignored by list-tabs, filters, and session tracking (protocol 0.4.0).
    - Enable CDP Network domain on attach (and re-enable after self-heal reattach) so HAR captures traffic in extension bridge mode.
  - **Config + reliability fixes**:
    - Surface config-load errors instead of silently falling back to prod.
    - Fix `CdpSession::close` leaving event subscribers hanging and HAR base64 size check OOM risk.
    - Guard `wait --relaxed-idle` with a pending-request count.
    - Windows: replace `wait_timeout` with a `try_wait` loop for cross-platform test reliability.
  - **Refactor**: extract `api_base` helpers

## 1.4.3

### Patch Changes

- Restore and harden extension mode commands.

  - Restore `extension install`, `extension uninstall`, `extension path`, `extension status`, and `extension ping` commands. Extension files are now bundled directly in the CLI binary at compile time — no network dependency, version always matches the CLI build.
  - Protocol 0.3.0: concurrent multi-tab debug. Each CDP request carries a root-level `tabId`; bridge and extension route by it. Removes implicit "active tab" default — `browser start --mode extension` now requires `--open-url` or `--tab-id`. Older extension (0.2.x) is hard-rejected with a reload prompt.
  - Expand CDP allowlist by 14 methods (DOM.resolveNode, DOM.describeNode, Runtime.callFunctionOn, Accessibility.queryAXTree, etc.) that were silently rejected, fixing every ref-based `click`/`inspect` call.
  - `browser open` in extension mode now uses `Extension.createTab` instead of the blocked `Target.createTarget`.
  - Add bridge health check on `browser start --mode extension` so the command waits up to 5s for the extension to reconnect after a daemon restart.
  - Fix `browser close-tab` in extension mode to detach only the target tab, not all attached tabs.
  - Fix `browser restart` in extension mode to avoid `MISSING_TAB_TARGET` when the previous session URL was filtered out.

## 1.4.2

### Patch Changes

- [#518](https://github.com/actionbook/actionbook/pull/518) [`2d4fc6d`](https://github.com/actionbook/actionbook/commit/2d4fc6daa80fe9d764e07f7bd19f4eb898dcc619) Thanks [@Senke0x](https://github.com/Senke0x)! - Harden extension bridge startup and surface actionable error hints.

  - Retry bridge port bind with exponential backoff (up to ~8.6s) to recover from `TIME_WAIT` after daemon restart
  - Move bridge bind to a background task so daemon cold start is no longer blocked by extension port contention (local/cloud modes were incorrectly delayed)
  - Add `BridgeListenerStatus` (Binding / Listening / Failed) so `browser start --mode extension` can distinguish still-binding from bind-failed
  - Wait up to 5s (polling every 100ms) for the extension to reconnect on `browser start`, eliminating a race where the bridge bound slightly before the extension completed its WS handshake
  - Surface `chrome://`, `devtools://` and other restricted schemes as `RESTRICTED_ACTIVE_TAB` with hint `pass --open-url <url>` instead of an opaque debugger.attach failure
  - Close `CdpSession` WebSocket gracefully on failed session start (writer sends a Close frame, session awaits the writer task) so the bridge sees EOF and releases its 1:1 gate — previously the next start attempt would instantly fail with `SessionClosed`
  - Print `hint:` line for Fatal/Retryable/UserAction error results in text output (previously only `--json` surfaced the hint field)

- [#518](https://github.com/actionbook/actionbook/pull/518) [`2d4fc6d`](https://github.com/actionbook/actionbook/commit/2d4fc6daa80fe9d764e07f7bd19f4eb898dcc619) Thanks [@Senke0x](https://github.com/Senke0x)! - Add Hermes agent integration.

  - New `SetupTarget::Hermes` variant invokes `hermes skills install` directly instead of routing through `npx skills add`, targeting Hermes's native skill registry at `~/.hermes/skills/`
  - Missing-binary error now points users to install Hermes (not Node.js) when the target is Hermes
  - Post-install verification parses `hermes skills list` table columns exactly (by `│` delimiter) to avoid false positives from similarly-named skills
  - `skills/actionbook/SKILL.md` gains Hermes-compatible frontmatter (`metadata.hermes.tags`, `requires_toolsets: [terminal]`, `prerequisites.commands: [actionbook]`) so the skill is discoverable via `hermes skills search` and hidden on non-terminal platforms

## 1.4.1

### Patch Changes

- [#511](https://github.com/actionbook/actionbook/pull/511) [`07a53a1`](https://github.com/actionbook/actionbook/commit/07a53a1c06a1036a9c121c2037dc5b88e33eb8a0) Thanks [@mcfn](https://github.com/mcfn)! - Fix Windows Chrome process cleanup and improve orphan recovery.

  - Replace NtQueryInformationProcess/ToolHelp with Win32 Job Objects for atomic termination of Chrome helper processes (renderer, GPU, utility)
  - Delete Chrome singleton lock files before orphan kill so new sessions can start even if helper processes linger
  - Add actionable error message when orphan Chrome still holds the user-data-dir lock
  - Fix 54 Windows e2e test cross-platform compatibility issues

## 1.4.0

### Minor Changes

- [#507](https://github.com/actionbook/actionbook/pull/507) [`5bddf13`](https://github.com/actionbook/actionbook/commit/5bddf13420a872355216b045ff616c6f58761feb) Thanks [@Senke0x](https://github.com/Senke0x)! - Add cloud browser provider support via `-p / --provider`.

  Supported providers: Driver, Hyperbrowser, Browseruse. Each provider reads its own `<PROVIDER>_API_KEY` from the caller's shell, and `browser restart` mints a fresh remote session while preserving the local `session_id`.

## 1.3.1

### Patch Changes

- Fix `browser list-tabs` in extension mode by using `Extension.listTabs` for live tab enumeration instead of `Target.getTargets`, which is not available in extension-level CDP access.

## 1.3.0

### Minor Changes

- [#499](https://github.com/actionbook/actionbook/pull/499) [`3092c61`](https://github.com/actionbook/actionbook/commit/3092c612dc25ff5a71d8fb361a50afc762fd9f09) Thanks [@Senke0x](https://github.com/Senke0x)! - Improve `actionbook setup` with a new skills installation step and targeted quick mode.

  - add a fifth setup step to install Actionbook skills during setup
  - add `actionbook setup --target <agent>` quick mode for one-shot skills installation
  - improve extension-mode setup guidance with Chrome Web Store and GitHub Releases fallback instructions
  - make API key input visible by default during interactive setup
  - tighten setup failure handling so quick mode and JSON flows report skills install failures correctly

## 1.2.0

### Minor Changes

- [#486](https://github.com/actionbook/actionbook/pull/486) [`86905eb`](https://github.com/actionbook/actionbook/commit/86905ebe112bd3c58f2db315920213e98c6b458f) Thanks [@Senke0x](https://github.com/Senke0x)! - Restore extension browser mode with WebSocket bridge relay

  - Add extension bridge server (ws://127.0.0.1:19222) for transparent CDP relay between Chrome extension and CLI daemon
  - Use Extension API (listTabs, attachTab, createTab, detachTab) for tab lifecycle management
  - Read default browser mode from config.toml instead of hardcoding Local
  - Fix build.rs to track git ref files in worktrees for accurate BUILD_VERSION
  - Add Local mode guard to prevent silent fallback from unsupported modes
  - Reject concurrent CDP clients in bridge to prevent response channel hijacking

## 1.1.0

### Minor Changes

- [#483](https://github.com/actionbook/actionbook/pull/483) [`4d46f8d`](https://github.com/actionbook/actionbook/commit/4d46f8d38f63a5ccc3f901db2733b3ec76e1c297) Thanks [@4bmis](https://github.com/4bmis)! - support create multi tabs in one shot

## 1.0.2

### Patch Changes

- Remove search and get commands from CLI help output and skill documentation

## 1.0.1

See 1.0.0 release notes below.

## 1.0.0

### Patch Changes

- [#464](https://github.com/actionbook/actionbook/pull/464) [`cc58aee`](https://github.com/actionbook/actionbook/commit/cc58aeebbc8456efa3c457ce6184de13d7971f92) Thanks [@mcfn](https://github.com/mcfn)! - Actionbook v1.0.0 — Browser Engine for AI Agents

  Rebuilt browser automation runtime designed for AI agents. Stateless session model, agent-friendly command interface, expanded command surface, and improved stability.

  ### Breaking Changes

  - **Stateless session model.** CLI now requires explicit `--session` and `--tab` flags for browser commands. Stateless interface, stateful runtime — agents reason about browser state through explicit addressing, not hidden side effects.

  ### Design: Agent-First CLI

  This is not a browser automation tool adapted for agents — it is built for agents from the ground up.

  - **Structured, parseable output.** Supports JSON via --json, with stable text output by default. Both formats are part of the formal contract.
  - **Predictable command surface.** Consistent argument patterns and return shapes across all 50+ commands. Agents don't need per-command special-casing.
  - **Stateless by default.** Explicit `--session` and `--tab` addressing means agents manage state in their own context window, not in hidden browser-side state. This maps directly to how LLM agents work — every call is self-contained.
  - **Snapshot refs as stable handles.** Elements are labeled with refs (`@e3`, `@e7`) that persist across commands within a snapshot. Agents can plan multi-step interactions without re-observing the page after every action.

  ### New Features

  - **50+ browser commands.** Expanded from the previous command set to cover the full browser surface: sessions, tabs, navigation, observation, interaction, waits, cookies, storage, queries, screenshots, PDF, uploads, and console logs.
  - **Multi-tab execution.** First-class `--tab` addressing. Run work across dozens of tabs in parallel.
  - **Snapshot refs.** `snapshot` labels every element with a stable ref. Chain multiple commands without re-snapshotting.

  ### Improvements

  - Expanded and hardened existing browser commands — more consistent argument handling, better error messages, and predictable return values.
  - 10× faster automation on complex sites via action manuals and snapshot ref chaining.
  - Full rebuild of CLI internals around stateless session/tab model.

  ### Bug Fixes

  - Fixed session cleanup failures causing orphaned browser processes.
  - Fixed tab addressing race conditions under parallel execution.
  - Resolved inconsistent snapshot refs across navigation events.
  - Improved error handling for browser commands on dynamic pages (SPAs, virtual DOMs, streaming components).

  ### Examples

  Repo ships with end-to-end examples: 192-site tagline collection (3 min), deep research report generation, and more. See `examples/`.

  ### Install

  ```
  npm install -g @actionbookdev/cli
  npx skills add actionbook/actionbook
  ```

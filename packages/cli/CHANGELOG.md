# @actionbookdev/cli

## 0.11.7

### Patch Changes

- [#318](https://github.com/actionbook/actionbook/pull/318) [`15c2154`](https://github.com/actionbook/actionbook/commit/15c21541204db88f7604490f8effdffd27ee68db) Thanks [@asensagent](https://github.com/asensagent)! - Add explicit JavaScript dialog support to the browser CLI, including dialog status, accept, and dismiss commands plus daemon warnings when a dialog is blocking the page.

## 0.11.6

### Patch Changes

- [#305](https://github.com/actionbook/actionbook/pull/305) [`53b1603`](https://github.com/actionbook/actionbook/commit/53b1603466d7db14c9cca53a66982b1b8e7dc2e5) Thanks [@ZhangHanDong](https://github.com/ZhangHanDong)! - Fix screenshot CDP timeout: extend daemon timeout from 30s to 120s for Page.captureScreenshot and Page.printToPDF, and add timeout-only fallback to direct WS for idempotent read-only methods

## 0.11.5

### Patch Changes

- [#244](https://github.com/actionbook/actionbook/pull/244) [`007aebe`](https://github.com/actionbook/actionbook/commit/007aebea578d0c3045ff1e0ece1411343c088800) Thanks [@mcfn](https://github.com/mcfn)! - Fix browser close failing with "TLS support not compiled in" on headerless wss:// CDP endpoints (e.g. Hyperbrowser)

- [#247](https://github.com/actionbook/actionbook/pull/247) [`3979ade`](https://github.com/actionbook/actionbook/commit/3979adee4a343f5670184e841607d243523f06e0) Thanks [@mcfn](https://github.com/mcfn)! - Fix daemon connect verification for remote WSS endpoints (e.g. Hyperbrowser) by routing verification through daemon instead of direct preflight probe

## 0.11.4

### Patch Changes

- [`6b81fee`](https://github.com/actionbook/actionbook/commit/6b81feee78762daee0f2ec62b833119ca6ca8b85) Thanks [@mcfn](https://github.com/mcfn)! - Fix browser close failing with "TLS support not compiled in" on headerless wss:// CDP endpoints (e.g. Hyperbrowser)

## 0.11.3

### Patch Changes

- [#238](https://github.com/actionbook/actionbook/pull/238) [`259150b`](https://github.com/actionbook/actionbook/commit/259150b6375dca5b958659b4e1c04d181078210f) Thanks [@mcfn](https://github.com/mcfn)! - Fix daemon mode liveness probes conflicting with single-connection WSS endpoints (e.g. AgentCore)

## 0.11.2

### Patch Changes

- [#235](https://github.com/actionbook/actionbook/pull/235) [`90ca8a1`](https://github.com/actionbook/actionbook/commit/90ca8a1625499504117fb67b517b48f89cd7810f) Thanks [@ZhangHanDong](https://github.com/ZhangHanDong)! - Fix external session reuse and improve documentation

  - Fix: external session reuse for `browser open` (CUE-703)
  - Docs: comprehensive browser automation guide with multi-session examples
  - Docs: mark CDP-only commands (wait-idle, console, fetch, emulate, switch-frame) in guide
  - Docs: correct fill command argument order (text first, selector second)

## 0.11.1

### Patch Changes

- [`4964fd5`](https://github.com/actionbook/actionbook/commit/4964fd54639045af62d3a865d3328a11d1c950c6) Thanks [@mcfn](https://github.com/mcfn)! - fix: Windows build failure — wrap daemon calls in cfg(unix) for session list/destroy commands

## 0.11.0

### Minor Changes

- [#230](https://github.com/actionbook/actionbook/pull/230) [`f5eea78`](https://github.com/actionbook/actionbook/commit/f5eea781b7243085b8747a37ad8cbf536684dbef) Thanks [@ZhangHanDong](https://github.com/ZhangHanDong)! - Add multi-session support for parallel tab operations

  - New `-S` / `--session` global flag to name sessions (e.g. `-S work`, `-S mail`)
  - Each named session binds to its own tab within a single browser process
  - Session commands: `browser session list|active|destroy <name>`
  - Session file naming: `{profile}@{session}.json` with auto-migration from legacy format
  - Daemon routes commands to correct tab per session via lazy attach
  - Fix: deterministic page persistence using known page ID after `browser open`
  - Fix: forked sessions inherit parent's active tab instead of falling back to arbitrary first page
  - Fix: `Target.createTarget` correctly routed through browser-level WebSocket (no sessionId)

## 0.10.1

### Patch Changes

- Fix `browser open` for remote CDP sessions with handshake headers, including the 30s create-target timeout and `--stealth` handling for newly opened tabs.

## 0.10.0

### Minor Changes

- [#211](https://github.com/actionbook/actionbook/pull/211) [`bd60552`](https://github.com/actionbook/actionbook/commit/bd60552819a56b0b6ad2b089b3e9d2dc5b6478fe) Thanks [@ZhangHanDong](https://github.com/ZhangHanDong)! - Add per-profile daemon with persistent WebSocket connection for CDP operations. Daemon is enabled by default on Unix+CDP mode, eliminating connect-per-command overhead. Use `--no-daemon` to opt out. New commands: `daemon status`, `daemon stop`.

## 0.9.2

### Patch Changes

- [`340d683`](https://github.com/actionbook/actionbook/commit/340d6835c6cee3198086990b0d1a82e9dae1ea48) Thanks [@mcfn](https://github.com/mcfn)! - Support remote wss:// CDP endpoints with optional auth headers

  - Fix: remote wss endpoints no longer fall back to localhost /json/list
  - Add `-H/--header` flag to `browser connect` for authenticated WebSocket endpoints
  - Session liveness, page enumeration, and CDP commands now work correctly over remote ws/wss

## 0.9.1

### Patch Changes

- [#195](https://github.com/actionbook/actionbook/pull/195) [`b173b12`](https://github.com/actionbook/actionbook/commit/b173b122f17a9fa40897e1ea8bc6a09dbb250a1b) Thanks [@Senke0x](https://github.com/Senke0x)! - Fix glibc compatibility for Debian 12 and Ubuntu 22.04 by pinning the linux-x64 build runner to ubuntu-22.04 (glibc 2.35), resolving "GLIBC_2.39 not found" errors on systems with glibc < 2.39

## 0.9.0

### Minor Changes

- [#190](https://github.com/actionbook/actionbook/pull/190) [`af5cd35`](https://github.com/actionbook/actionbook/commit/af5cd3522a43aaa1906e422ef92e2aa290dfc293) Thanks [@ZhangHanDong](https://github.com/ZhangHanDong)! - Add Electron app automation and pydoll-inspired browser automation features

  **Electron App Automation:**

  - New `actionbook app` command for automating Electron desktop apps (VS Code, Slack, Discord, Figma, Notion, Spotify)
  - Auto-discover and launch apps with `app launch <name>`
  - Connect to running apps with `app attach <port>`
  - Full feature parity with browser commands (all 35+ commands work with app prefix)

  **Pydoll-Inspired Browser Features:**

  - Shadow DOM Support: Use `::shadow-root` selector syntax to interact with web components
  - IFrame Context Switching: Switch between main frame and iframes with `browser switch-frame`
  - Keyboard Hotkeys: Send keyboard combinations with `browser hotkey "Control+C"`
  - Scroll with Wait: Wait for scrollend event with `browser scroll down 500 --wait`

  **Bug Fixes:**

  - Fix iframe context switching to properly execute in target frame using CDP isolated worlds
  - Fix extension close command to prevent zombie processes
  - Fix Windows wildcard path expansion for app discovery

  All browser automation features are available in both `browser` and `app` commands.

## 0.8.3

### Patch Changes

- [#179](https://github.com/actionbook/actionbook/pull/179) [`a259b6d`](https://github.com/actionbook/actionbook/commit/a259b6d25560c7eaa2b66f6075dc5938a344086e) Thanks [@Senke0x](https://github.com/Senke0x)! - Fix CWS extension ID mismatch and browser close bridge lifecycle:

  - Support Chrome Web Store extension ID alongside dev extension ID for origin validation and native messaging
  - Remove misleading port change suggestion from bridge conflict error message
  - `browser close --extension` now fully cleans up bridge lifecycle: best-effort tab detach → stop bridge process → delete all state files (PID, port, token)

## 0.8.1

### Patch Changes

- [#173](https://github.com/actionbook/actionbook/pull/173) [`a68fb6a`](https://github.com/actionbook/actionbook/commit/a68fb6a06f9fec17f440541a34464c308237ff03) Thanks [@Senke0x](https://github.com/Senke0x)! - Fix extension mode connectivity and harden bridge security:

  - Unify extension commands through `ExtensionBackend` with 30-second connection retry, fixing immediate "Extension not connected" failure when Chrome extension needs 2-6s to connect via Native Messaging
  - Restrict extension bridge auth to exact Actionbook extension ID (`native_messaging::EXTENSION_ID`), preventing other Chrome extensions from impersonating the bridge client
  - Harden extension bridge against spoofing and PID race conditions
  - Fix extension disconnect race, PID overflow guard, and bridge port constant
  - Resolve PID lifecycle, SIGKILL safety, mode priority, and config preservation bugs
  - Restore extension mode end-to-end pipeline and v0.7.5 setup wizard compatibility

## 0.8.0

### Minor Changes

- [#170](https://github.com/actionbook/actionbook/pull/170) [`0329b54`](https://github.com/actionbook/actionbook/commit/0329b544b878b60d39c1bdcc0433452dd9f2ea79) Thanks [@ZhangHanDong](https://github.com/ZhangHanDong)! - Release actionbook-rs 0.8.0

  - Feature I1-I5: One-shot fetch, HTTP-first degradation, session tag tracking, URL rewriting, domain-aware wait
  - Feature J1: File upload support (DOM.setFileInputFiles + React SPA compatible)
  - Extended selector support: Playwright-style `:has-text()` and `:nth(N)` pseudo-selectors
  - Improved error handling and verification patterns

## 0.7.5

### Patch Changes

- [#159](https://github.com/actionbook/actionbook/pull/159) [`6ad3b57`](https://github.com/actionbook/actionbook/commit/6ad3b5708af1b16548c61e9f60121f72368229e5) Thanks [@Senke0x](https://github.com/Senke0x)! - Refine `actionbook setup` behavior for agent and non-interactive workflows:

  - remove `--agent-mode` and keep setup targeting via `--target`
  - keep `--target` quick mode only when used alone
  - run full setup when `--target` is combined with setup flags (for example `--non-interactive`, `--browser`, `--api-key`)
  - avoid forcing non-interactive/browser defaults from `--target`
  - preserve standalone target behavior by skipping skills integration in full setup
  - improve setup help text with agent-friendly non-interactive examples

## 0.7.4

### Patch Changes

- [#153](https://github.com/actionbook/actionbook/pull/153) [`defe7f8`](https://github.com/actionbook/actionbook/commit/defe7f88ff401ba1bf6c2043479039d37dc0d255) Thanks [@adcentury](https://github.com/adcentury)! - Add a simple welcome screen to `actionbook setup` showing the Actionbook logo and name.

## 0.7.3

### Patch Changes

- [#135](https://github.com/actionbook/actionbook/pull/135) [`deedfe8`](https://github.com/actionbook/actionbook/commit/deedfe8836c56ac3b48123989405afd84a06bad7) Thanks [@4bmis](https://github.com/4bmis)! - Use changesets to manage packages

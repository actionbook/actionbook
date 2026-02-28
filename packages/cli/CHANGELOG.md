# @actionbookdev/cli

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

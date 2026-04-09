# @actionbookdev/cli

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

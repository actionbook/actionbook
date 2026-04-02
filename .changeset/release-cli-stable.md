---
"@actionbookdev/cli": patch
---

Actionbook v1.0.0 — Browser Engine for AI Agents

Rebuilt browser automation runtime designed for AI agents. Stateless session model, agent-friendly command interface, expanded command surface, and improved stability.

### Breaking Changes

- **Stateless session model.** CLI now requires explicit `--session` and `--tab` flags for all browser commands. All state is caller-managed — no implicit browser state, no cross-call side effects. Agents can reason about browser state without tracking hidden side effects.

### Design: Agent-First CLI

This is not a browser automation tool adapted for agents — it is built for agents from the ground up.

- **Structured, parseable output.** Every command returns JSON. No human-oriented formatting for agents to parse around.
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
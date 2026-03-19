---
"@actionbookdev/cli": minor
---

Add multi-session support for parallel tab operations

- New `-S` / `--session` global flag to name sessions (e.g. `-S work`, `-S mail`)
- Each named session binds to its own tab within a single browser process
- Session commands: `browser session list|active|destroy <name>`
- Session file naming: `{profile}@{session}.json` with auto-migration from legacy format
- Daemon routes commands to correct tab per session via lazy attach
- Fix: deterministic page persistence using known page ID after `browser open`
- Fix: forked sessions inherit parent's active tab instead of falling back to arbitrary first page
- Fix: `Target.createTarget` correctly routed through browser-level WebSocket (no sessionId)

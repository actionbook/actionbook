---
"@actionbookdev/cli": patch
---

Harden extension bridge startup and surface actionable error hints.

- Retry bridge port bind with exponential backoff (up to ~8.6s) to recover from `TIME_WAIT` after daemon restart
- Move bridge bind to a background task so daemon cold start is no longer blocked by extension port contention (local/cloud modes were incorrectly delayed)
- Add `BridgeListenerStatus` (Binding / Listening / Failed) so `browser start --mode extension` can distinguish still-binding from bind-failed
- Wait up to 5s (polling every 100ms) for the extension to reconnect on `browser start`, eliminating a race where the bridge bound slightly before the extension completed its WS handshake
- Surface `chrome://`, `devtools://` and other restricted schemes as `RESTRICTED_ACTIVE_TAB` with hint `pass --open-url <url>` instead of an opaque debugger.attach failure
- Close `CdpSession` WebSocket gracefully on failed session start (writer sends a Close frame, session awaits the writer task) so the bridge sees EOF and releases its 1:1 gate — previously the next start attempt would instantly fail with `SessionClosed`
- Print `hint:` line for Fatal/Retryable/UserAction error results in text output (previously only `--json` surfaced the hint field)

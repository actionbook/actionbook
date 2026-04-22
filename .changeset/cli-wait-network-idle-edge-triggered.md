---
"@actionbookdev/cli": patch
---

`wait network-idle` is now edge-triggered: pre-existing long-lived connections at the start of the wait (websockets, long-poll requests that opened before the command ran) are ignored. Previously these could keep the wait from ever resolving on pages with persistent background traffic.

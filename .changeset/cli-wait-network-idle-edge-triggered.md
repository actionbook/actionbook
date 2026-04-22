---
"@actionbookdev/cli": patch
---

`wait network-idle` is edge-triggered: long-lived connections opened before the wait started are ignored.

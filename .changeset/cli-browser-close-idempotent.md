---
"@actionbookdev/cli": patch
---

`browser close` is idempotent — when the session is already gone it returns ok with `meta.warnings: ["SESSION_ALREADY_GONE: ..."]` instead of a fatal error.

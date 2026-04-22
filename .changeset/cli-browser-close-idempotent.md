---
"@actionbookdev/cli": patch
---

`browser close` is now idempotent. When the target session is already gone (never started, or closed by another caller), the command returns success with `meta.warnings: ["SESSION_ALREADY_GONE: ..."]` instead of a fatal error. Cleanup scripts can call `browser close` unconditionally without having to first check session existence. A concurrent close for the same session still returns the fatal `SESSION_CLOSING` code (unchanged).

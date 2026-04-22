---
"@actionbookdev/cli": minor
---

`browser start --set-session-id` is get-or-create (alias for `--session`); reusing with a conflicting `--profile` returns the new `SESSION_PROFILE_MISMATCH` error code.

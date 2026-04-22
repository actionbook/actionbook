---
"@actionbookdev/cli": minor
---

`browser start --set-session-id <id>` is now get-or-create (functional alias for `--session <id>`). Previously it always created a new session and failed with `SESSION_ALREADY_EXISTS` when the ID was already running; scripts calling it twice for idempotent attach had to handle that error. Both flags now reuse a Running session with the given ID or create one if not found.

When reusing, passing `--profile <name>` that does not match the session's bound profile fails with the new `SESSION_PROFILE_MISMATCH` error code (retryable: false). `error.hint` explains the required profile; `error.details` includes `session_id`, `bound_profile`, and `requested_profile`. Omit `--profile` or pass the matching value to reuse successfully.

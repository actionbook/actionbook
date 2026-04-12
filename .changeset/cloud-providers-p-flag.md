---
"@actionbookdev/cli": minor
---

Add cloud browser provider support via `-p / --provider`.

Supported providers: Driver, Hyperbrowser, Browseruse. Each provider reads its own `<PROVIDER>_API_KEY` from the caller's shell, and `browser restart` mints a fresh remote session while preserving the local `session_id`.

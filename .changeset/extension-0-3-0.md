---
"@actionbookdev/extension": minor
---

Release 0.3.0: align extension bridge with Actionbook CLI 1.x.

- Support CLI 1.x stateless architecture — every message is self-contained with explicit `--session`/`--tab` addressing, no implicit current-tab state.
- Concurrent multi-tab operation: bridge protocol upgraded to handle parallel CDP traffic across multiple tabs in a single session.
- Health check on startup to prevent connect/disconnect loops.

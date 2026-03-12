---
"@actionbookdev/cli": patch
---

Support remote wss:// CDP endpoints with optional auth headers

- Fix: remote wss endpoints no longer fall back to localhost /json/list
- Add `-H/--header` flag to `browser connect` for authenticated WebSocket endpoints
- Session liveness, page enumeration, and CDP commands now work correctly over remote ws/wss

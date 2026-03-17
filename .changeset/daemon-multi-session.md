---
"@actionbookdev/cli": minor
---

Add per-profile daemon with persistent WebSocket connection for CDP operations. Daemon is enabled by default on Unix+CDP mode, eliminating connect-per-command overhead. Use `--no-daemon` to opt out. New commands: `daemon status`, `daemon stop`.

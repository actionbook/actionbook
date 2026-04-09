---
"@actionbookdev/cli": minor
---

Restore extension browser mode with WebSocket bridge relay

- Add extension bridge server (ws://127.0.0.1:19222) for transparent CDP relay between Chrome extension and CLI daemon
- Use Extension API (listTabs, attachTab, createTab, detachTab) for tab lifecycle management
- Read default browser mode from config.toml instead of hardcoding Local
- Fix build.rs to track git ref files in worktrees for accurate BUILD_VERSION
- Add Local mode guard to prevent silent fallback from unsupported modes
- Reject concurrent CDP clients in bridge to prevent response channel hijacking

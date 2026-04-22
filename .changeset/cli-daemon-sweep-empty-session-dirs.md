---
"@actionbookdev/cli": patch
---

Daemon now sweeps empty session directories and stale `__fetch_*.json` files at start and stop. Prevents `~/.actionbook/sessions/` from accumulating orphan directories when sessions exit abnormally.

---
"@actionbookdev/cli": patch
---

Fix screenshot CDP timeout: extend daemon timeout from 30s to 120s for Page.captureScreenshot and Page.printToPDF, and add timeout-only fallback to direct WS for idempotent read-only methods

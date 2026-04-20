---
"@actionbookdev/extension": patch
---

Simplify tab ownership model: `Extension.listTabs` now returns only tabs in the Actionbook group (drag in = appears, drag out = disappears). `Extension.attachTab` always moves the tab into the group. Remove the unused `ACTIONBOOK_GROUP_ATTACH` flag.

---
"@actionbookdev/cli": minor
---

Add `HYPERBROWSER_PROXY_COUNTRY` env var to pin the Hyperbrowser proxy exit region (ISO country code, e.g. `JP`). Defaults to `US` when unset, so every Hyperbrowser session now includes an explicit `proxyCountry` in the create-session request.

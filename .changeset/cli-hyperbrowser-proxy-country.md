---
"@actionbookdev/cli": minor
---

Add optional `HYPERBROWSER_PROXY_COUNTRY` env var to pin the Hyperbrowser proxy exit region (ISO country code, e.g. `US`, `JP`). Forwarded as `proxyCountry` on the create-session request; when unset, the field is omitted and Hyperbrowser's own default applies. Matches the existing per-provider pattern (`DRIVER_DEV_COUNTRY`, `BROWSER_USE_PROXY_COUNTRY_CODE`).

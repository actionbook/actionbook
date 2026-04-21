---
"@actionbookdev/cli": minor
---

Add proxy-country configuration to the Hyperbrowser provider and introduce a cross-provider fallback env var.

- New vendor-specific env var `HYPERBROWSER_PROXY_COUNTRY` — forwarded as `proxyCountry` on Hyperbrowser's create-session request. When unset, the field is omitted so Hyperbrowser's own default applies.
- New cross-provider env var `ACTIONBOOK_PROXY_COUNTRY` — acts as a fallback for all three providers when their vendor-specific name isn't set. An agent can now set one variable and have it map to `proxyCountry` (Hyperbrowser), `proxyCountryCode` (Browser Use), or `country` (driver.dev) automatically. Vendor-specific env vars still take precedence.

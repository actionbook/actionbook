---
"@actionbookdev/cli": minor
---

CDP errors surface as six classified codes (`CDP_NAV_TIMEOUT`, `CDP_NOT_INTERACTABLE`, `CDP_NODE_NOT_FOUND`, `CDP_TARGET_CLOSED`, `CDP_PROTOCOL_ERROR`, `CDP_GENERIC`), with `CDP_NAV_TIMEOUT` and `CDP_TARGET_CLOSED` now `retryable: true`.

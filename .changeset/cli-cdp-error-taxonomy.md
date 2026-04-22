---
"@actionbookdev/cli": minor
---

Structured CDP error taxonomy. CDP failures now surface through a fixed set of codes in `error.code` — `CDP_NAV_TIMEOUT`, `CDP_NOT_INTERACTABLE`, `CDP_NODE_NOT_FOUND`, `CDP_TARGET_CLOSED`, `CDP_PROTOCOL_ERROR`, `CDP_GENERIC` — each with a stable `retryable` flag and `error.details` containing `cdp_code` (upstream numeric code) and `reason` (raw message). Replaces the previous single `CDP_ERROR` literal for classified cases.

Behavior change: `CDP_NAV_TIMEOUT` and `CDP_TARGET_CLOSED` now report `retryable: true` (previously all CDP errors were `retryable: false`). Callers that key retry decisions off the envelope's `retryable` flag should verify the new behavior matches their expectations. Fifteen-plus hand-crafted `CDP_ERROR` literals remain as a legacy fallback and will migrate in a follow-up.

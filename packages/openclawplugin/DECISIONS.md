# Decisions for CUE-671

This file records the key design decisions already made for the Actionbook → OpenClaw integration package.

## Confirmed decisions

### 1. Formal integration shape: OpenClaw plugin

**Decision:** use an OpenClaw plugin as the formal integration boundary.

**Why:**
- it matches OpenClaw’s native extension model
- it supports first-class tools, policy, config, and lifecycle integration
- it avoids relying on shell-only or prompt-only integration

---

### 2. MVP tool scope: search + get only

**Decision:** the first planned package version should expose only:
- `actionbook_search`
- `actionbook_get`

**Why:**
- these are the highest-value low-token query capabilities
- they keep the package focused on knowledge retrieval
- they avoid premature expansion into discovery or browser-aware logic

---

### 3. Explicit exclusions from MVP

**Decision:** the MVP does not include:
- `list_sources`
- `search_sources`
- browser execution
- selector -> `ref(uid)` bridge
- browser hook integration

**Why:**
- PM explicitly narrowed scope
- these would widen the surface too early
- browser-aware integration requires a separate phase and clearer execution contracts

---

### 4. Internal backend preference: SDK first

**Decision:** the future package should use `@actionbookdev/sdk` as the primary internal backend.

**Why:**
- shortest internal dependency path
- simpler than shelling out to CLI
- simpler than adding an extra MCP transport layer
- better base for future structured outputs

---

### 5. Output contract preference: structured and deterministic

**Decision:** future tool outputs should be normalized into deterministic, machine-friendly payloads.

**Why:**
- OpenClaw integration should prefer stable tool contracts over loose prose
- deterministic formatting is easier to test
- it keeps the package ready for later browser-aware extension work

---

### 6. Product boundary: Actionbook is knowledge layer, not execution layer

**Decision:** Actionbook should not replace OpenClaw browser execution.

**Why:**
- OpenClaw already owns browser control and transport routing
- replacing execution would create duplicate session/tab/error handling models
- Actionbook’s best leverage is token-efficient knowledge retrieval

---

### 7. Current branch deliverable: documents only

**Decision:** for the current branch state, deliver only Markdown documentation.

**Why:**
- PM explicitly paused code implementation
- the current requirement is to preserve branch + planning package directory + written design/test documentation

---

## Deferred decisions

These are intentionally postponed until implementation resumes:

1. exact package exports and manifest shape
2. concrete OpenClaw runtime type dependency strategy
3. exact normalized output schema for every response field
4. whether the future package should emit JSON text or richer OpenClaw-native payloads
5. how later browser-aware bridge logic should be split between Actionbook side and OpenClaw side

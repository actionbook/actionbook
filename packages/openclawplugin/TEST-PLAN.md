# Test Plan for packages/openclawplugin

This file defines the intended test strategy for the future implementation of the Actionbook → OpenClaw plugin package.

## Current status

This is a planning artifact only. No tests are implemented yet because PM has paused code work for the package.

---

## 1. Test goals

The future implementation should prove all of the following:

1. the plugin registers exactly the intended tools
2. search/get input validation is enforced correctly
3. Actionbook SDK calls are wired correctly
4. formatted outputs are deterministic and machine-friendly
5. failures are normalized into stable tool errors
6. the package is buildable and testable in isolation

---

## 2. Planned test categories

## 2.1 Plugin registration tests

### Goal
Verify that the plugin registration layer exposes the correct tools and no more.

### Planned cases
1. registers `actionbook_search`
2. registers `actionbook_get`
3. does not register `list_sources`
4. does not register `search_sources`
5. registration still succeeds with empty plugin config

---

## 2.2 Search input validation tests

### Goal
Ensure future `actionbook_search` rejects invalid inputs predictably.

### Planned cases
1. missing `query` returns validation error
2. empty `query` returns validation error
3. invalid `page` type returns validation error
4. invalid `page_size` type returns validation error
5. invalid `page_size` bounds return validation error
6. valid optional fields (`domain`, `background`, `url`) pass through

---

## 2.3 Get input validation tests

### Goal
Ensure future `actionbook_get` enforces `area_id` correctly.

### Planned cases
1. missing `area_id` returns validation error
2. empty `area_id` returns validation error
3. valid `area_id` passes

---

## 2.4 SDK adapter tests

### Goal
Verify the package talks to `@actionbookdev/sdk` correctly.

### Planned cases
1. search tool forwards all supported search fields correctly
2. get tool forwards the exact `area_id`
3. package config (`apiKey`, `baseUrl`, `timeoutMs`) is passed into client creation correctly
4. client factory can be swapped/mocked cleanly in tests

---

## 2.5 Formatting tests

### Goal
Ensure normalized outputs stay deterministic.

### Planned cases
1. search output formatting produces stable field order / stable structure
2. get output formatting produces stable field order / stable structure
3. empty search results are explicit rather than ambiguous
4. formatter output remains compact and machine-oriented

---

## 2.6 Error handling tests

### Goal
Ensure upstream failures become predictable tool responses.

### Planned cases
1. Actionbook SDK typed error -> normalized Actionbook tool error
2. generic runtime error -> normalized internal error
3. empty upstream response -> explicit fallback handling
4. malformed upstream response -> explicit fallback handling

---

## 2.7 Package wiring tests

### Goal
Catch packaging mistakes early once implementation starts.

### Planned cases
1. package can build in isolation
2. package tests run in isolation
3. package export points resolve correctly
4. plugin manifest exists and matches the intended package role

---

## 3. Proposed future verification commands

When implementation resumes, the intended verification commands are:

```bash
pnpm --filter @actionbookdev/openclawplugin build
pnpm --filter @actionbookdev/openclawplugin test
```

Optional follow-up checks:

```bash
git status
pnpm build --filter @actionbookdev/openclawplugin...
```

---

## 4. Non-goals for the test plan

These are not part of the future MVP test matrix for this issue:

- browser execution tests
- Chrome MCP integration tests
- selector -> ref(uid) bridge tests
- source/resource listing tests
- end-to-end current-browser automation tests

Those belong to later phases if and when implementation scope expands.

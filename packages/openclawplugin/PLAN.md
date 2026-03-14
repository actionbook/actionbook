# OpenClaw Plugin Plan for Actionbook (CUE-671)

## Current status

- Branch: `cue-671-openclawplugin`
- Current phase: **design only**
- PM latest instruction: **do not do code development yet**
- Deliverable in this branch: background + research conclusions + development plan + test plan in Markdown only

---

## 1. Task background

The goal is to define how Actionbook should integrate with OpenClaw after OpenClaw v2026.3.13 introduced stronger browser control capabilities, including attach-to-current-browser support through Chrome DevTools MCP.

The original question was not simply “how to expose Actionbook in OpenClaw”, but more specifically:

1. How OpenClaw currently controls the browser
2. Whether Actionbook should integrate through MCP, plugin, skill, or CLI
3. How Actionbook can provide value **without competing on browser control**
4. How to minimize token usage while still improving execution quality

After research and PM clarification, the target was narrowed to:

- create a new branch
- plan a package under `packages/openclawplugin`
- only consider two first-class capabilities:
  - `search`
  - `get`
- do **not** implement code yet

---

## 2. Research conclusions

### 2.1 OpenClaw browser control architecture

OpenClaw browser control is a **first-party core capability**, not primarily implemented by skill or external plugin.

It currently routes browser control through multiple profiles/transports:

- `openclaw`: managed browser session
- `user`: existing browser session via official Chrome DevTools MCP attach
- `chrome-relay`: OpenClaw extension relay path

Important architectural conclusion:

- OpenClaw should remain the **execution layer**
- Actionbook should become the **knowledge/query layer**

### 2.2 Why Actionbook should not replace OpenClaw browser execution

Actionbook also has browser-related surfaces (CLI / extension / automation helpers), but replacing OpenClaw browser execution would create two overlapping stacks:

- two session models
- two tab/state management models
- two error recovery paths
- two different notions of “current browser”

This would be high-cost and confusing.

### 2.3 Why plugin is the preferred formal integration point

Compared with MCP-only or skill+CLI-only approaches, an OpenClaw plugin is preferred because it:

1. fits OpenClaw’s official extension model
2. can register first-class tools
3. participates in tool policy / config / lifecycle management
4. can evolve later into deeper integration if needed
5. avoids building a second execution stack

### 2.4 Why MCP alone is not sufficient for the long-term design

Current Actionbook MCP is useful, but it is mainly **query-oriented** and not sufficient as the only long-term integration surface.

MCP is good for:

- IDE integration
- external agent hosts
- quick query exposure

MCP is not enough for future deeper OpenClaw integration because:

- it is not aware of OpenClaw live browser context
- it is not the native OpenClaw extension boundary
- future browser-aware bridge logic would still need an OpenClaw-native integration point

### 2.5 Why skill + CLI is not the final answer

Skill + CLI is viable as a prototype path, but not ideal as the formal product path.

It is weaker because:

- it depends on shelling out
- stdout parsing is less stable than native tool contracts
- observability is worse
- policy/control is weaker than native OpenClaw tools

### 2.6 Product positioning conclusion

The best product split is:

- **OpenClaw** = browser execution plane
- **Actionbook** = low-token task knowledge plane

This means Actionbook should first contribute:

- search of action manuals
- get manual details for a selected area
- structured, deterministic outputs that help the agent reason with less page exploration

---

## 3. Scope locked by PM

### In scope for MVP planning

Only two plugin-facing tools are planned:

1. `actionbook_search`
2. `actionbook_get`

### Explicitly out of scope

The following are intentionally **not** part of this phase:

- `list_sources`
- `search_sources`
- browser execution
- selector -> live `ref(uid)` bridge
- OpenClaw browser route hooks
- OpenClaw runtime auto-routing by domain

---

## 4. Proposed package goal

`packages/openclawplugin` is intended to become an Actionbook-owned package that packages Actionbook capability for OpenClaw plugin consumption.

Its responsibility is:

- expose Actionbook as OpenClaw-friendly tools
- provide a stable config surface
- normalize Actionbook query results into deterministic machine-friendly outputs
- preserve a clean path for future extension

Its responsibility is **not**:

- controlling browsers directly
- replacing OpenClaw browser tools
- deciding OpenClaw agent strategy by itself

---

## 5. Package design plan

## 5.1 Intended package shape

Planned package layout:

```text
packages/openclawplugin/
  PLAN.md
  README.md                 # later, package usage docs
  package.json              # later
  openclaw.plugin.json      # later
  src/
    index.ts
    lib/
      client.ts
      formatters.ts
      errors.ts
    tools/
      search.ts
      get.ts
    __tests__/
      index.test.ts
      search.test.ts
      get.test.ts
      formatter.test.ts
      client.test.ts
```

### Notes

- `PLAN.md` exists now as the current non-code deliverable
- all code/config files above are **planned only**, not implemented in this phase

---

## 5.2 Internal dependency choice

The preferred internal backend is:

- `@actionbookdev/sdk`

Reasoning:

1. shortest path inside the monorepo
2. avoids an extra MCP transport layer
3. cleaner than CLI shell execution
4. better for future structured returns

Fallbacks that remain conceptually possible but not preferred:

- Actionbook MCP
- Actionbook CLI

---

## 5.3 Tool contract plan

### Tool 1: `actionbook_search`

#### Purpose
Search Actionbook manuals relevant to a task or website.

#### Planned inputs

```ts
{
  query: string,
  domain?: string,
  background?: string,
  url?: string,
  page?: number,
  page_size?: number
}
```

#### Planned output shape

Output should be deterministic and machine-friendly.

Proposed normalized shape:

```json
{
  "tool": "actionbook_search",
  "query": "airbnb search",
  "filters": {
    "domain": "airbnb.com",
    "url": null,
    "page": 1,
    "page_size": 10
  },
  "results": [
    {
      "area_id": "airbnb.com:/:default",
      "summary": "Search form and related controls",
      "domain": "airbnb.com",
      "health_hint": "high"
    }
  ]
}
```

### Tool 2: `actionbook_get`

#### Purpose
Get a specific action manual / area definition by `area_id`.

#### Planned inputs

```ts
{
  area_id: string
}
```

#### Planned output shape

```json
{
  "tool": "actionbook_get",
  "area_id": "airbnb.com:/:default",
  "manual": {
    "summary": "Search form and related actions",
    "elements": [
      {
        "name": "destination_input",
        "methods": ["type"],
        "selectors": {
          "css": "...",
          "xpath": "..."
        }
      }
    ]
  }
}
```

### Output design principles

- deterministic
- compact
- machine-friendly
- easy to test
- not loose prose as the primary contract

---

## 5.4 Config plan

Planned config surface:

```ts
{
  apiKey?: string,
  baseUrl?: string,
  timeoutMs?: number
}
```

Config goals:

- minimal
- easy to validate
- aligned with existing Actionbook SDK options

---

## 5.5 Future extension point reserved

A future phase may extend this package with browser-aware integration logic.

The most valuable future path is:

- use Actionbook manual output as a hint layer
- map manual selectors / semantic targets to OpenClaw live browser snapshot refs
- support selector -> `ref(uid)` bridge

This is explicitly **not** part of the current phase.

---

## 6. Development decisions already made

The following decisions are already settled for this issue unless PM changes scope again:

1. **Plugin is the preferred formal integration shape**
2. **Only search/get belong in the first phase**
3. **Actionbook does not replace OpenClaw browser execution**
4. **SDK is the preferred internal backend**
5. **Structured deterministic output is preferred over free-form text output**
6. **source/resource discovery tools are intentionally deferred**
7. **browser-aware bridge logic is intentionally deferred**

---

## 7. Development plan (no code yet)

## Phase A — package scaffolding

Planned tasks:

1. create package directory and metadata
2. add package build/test config
3. add OpenClaw plugin manifest
4. define minimal OpenClaw-compatible plugin-facing local types

Deliverable:
- package skeleton ready for implementation

## Phase B — tool implementation

Planned tasks:

1. implement `actionbook_search`
2. implement `actionbook_get`
3. add input validation
4. add deterministic result formatting
5. add error normalization

Deliverable:
- two functional plugin tools wired to Actionbook SDK

## Phase C — tests and validation

Planned tasks:

1. add unit tests
2. verify build in isolation
3. verify test execution in isolation
4. verify package exports / manifest sanity

Deliverable:
- package ready for review

---

## 8. Test plan

## 8.1 Registration tests

Purpose:
Verify that the plugin registers exactly the expected tools.

Cases:

1. plugin registers `actionbook_search`
2. plugin registers `actionbook_get`
3. no extra tools are registered
4. registration remains stable if config is omitted

## 8.2 Input validation tests

Purpose:
Ensure invalid tool input is rejected predictably.

### Search tests

1. missing `query` -> error
2. empty `query` -> error
3. invalid `page` / `page_size` -> error
4. valid optional filters pass through

### Get tests

1. missing `area_id` -> error
2. empty `area_id` -> error
3. valid `area_id` passes

## 8.3 SDK adapter tests

Purpose:
Ensure tool handlers call the Actionbook SDK correctly.

Cases:

1. search tool forwards query + optional filters correctly
2. get tool forwards `area_id` correctly
3. configured `apiKey` / `baseUrl` / `timeoutMs` are passed into client creation

## 8.4 Formatting tests

Purpose:
Ensure outputs remain deterministic and machine-friendly.

Cases:

1. search result formatting produces stable JSON text
2. get result formatting produces stable JSON text
3. empty search result formatting is explicit and stable
4. formatter does not depend on property insertion accidents

## 8.5 Error handling tests

Purpose:
Ensure failures are exposed consistently.

Cases:

1. SDK throws Actionbook error -> normalized tool error output
2. SDK throws generic error -> normalized internal error output
3. malformed upstream text / empty upstream text -> safe fallback formatting or explicit error

## 8.6 Package wiring tests

Purpose:
Catch packaging-level mistakes.

Cases:

1. package export path points to built index
2. manifest exists and includes config schema
3. build succeeds with monorepo filter
4. test succeeds with monorepo filter

---

## 9. Verification plan

When implementation starts later, the intended verification commands are:

```bash
pnpm --filter @actionbookdev/openclawplugin build
pnpm --filter @actionbookdev/openclawplugin test
```

Additional checks to run later:

```bash
git status
pnpm build --filter @actionbookdev/openclawplugin...
```

Current phase does **not** run implementation verification because no code should be added.

---

## 10. Risks and open questions

### Risks

1. Actionbook SDK currently returns text-oriented results in some flows, so plugin formatting needs a careful normalization layer.
2. OpenClaw plugin runtime types are not available in this repo, so a publishable package may need a lightweight compatibility contract or peer integration strategy.
3. If future requirements expand too early into browser-aware behavior, complexity will jump quickly.

### Open questions for later implementation

1. Should output stay as JSON string-in-text for maximum OpenClaw compatibility, or should the plugin later support richer content payloads?
2. Should OpenClaw-facing types live in this package, or should they be imported from an OpenClaw-compatible package later?
3. Should future bridge logic live in this package or in an OpenClaw-side companion integration layer?

---

## 11. Final design summary

For this issue, the planned package should be understood as:

- a **formal OpenClaw integration wrapper** owned by Actionbook
- focused on **query capabilities only**
- beginning with **search** and **get** only
- intentionally **not** competing with OpenClaw browser execution
- designed to become the base for a future, more advanced Actionbook -> OpenClaw integration path

This document is the source of truth for the current non-code planning phase.

---
name: actionbook-web-test
description: Run browser-based web tests against websites using Actionbook CLI. Activate when the user wants to test a website workflow, run smoke tests, verify a user flow, check if a web application works, run regression tests, or validate browser-based interactions. Supports test definition, execution, assertion, reporting, and json-ui visual report generation.
---

## When to Use This Skill

Activate when the user:

- Asks to "test", "verify", "check", or "validate" a website workflow
- Wants to run smoke tests or health checks on a web application
- Needs to verify a user flow works end-to-end (login, checkout, search, etc.)
- Asks to "run regression tests" or "does this still work?"
- Wants to confirm a deployment didn't break functionality
- Needs to monitor a website's functionality on a schedule
- Builds browser-based test suites without writing Playwright/Cypress code

## What actionbook-web-test Provides

actionbook-web-test transforms web tests from coded test scripts into **declarative YAML workflows** executed by AI agents via Actionbook CLI.

| Benefit | How |
|---------|-----|
| **AI-native recovery** | When a selector fails, the agent snapshots the live page and finds the equivalent element |
| **Actionbook-managed selectors** | Pre-verified selectors with health scores — no manual maintenance |
| **Cross-project reusability** | YAML workflows work anywhere Actionbook CLI is installed |
| **No test framework required** | No Playwright/Cypress/Jest setup — just `actionbook browser` commands |
| **Human-readable tests** | YAML workflows are readable by non-developers |
| **Visual test reports** | json-ui powered HTML reports with metrics, step details, and failure screenshots |

## Test Workflow Format

Tests are defined as YAML files in a `tests/` directory. Each file describes one test workflow.

```yaml
name: example-test
description: What this test verifies
url: https://example.com
tags: [smoke, critical]
timeout: 30000  # ms, default 30000

# Pre-fetch verified selectors from Actionbook
actions:
  - "example.com:/:default"

# Environment variables (support {{env.VAR}} templates)
env:
  USERNAME: "test-user"
  PASSWORD: "{{env.TEST_PASSWORD}}"

# Browser setup options
setup:
  headless: true
  auto_dismiss_dialogs: true
  no_animations: true

# Ordered test steps
steps:
  - name: Open page
    action: open
    url: "https://example.com"

  - name: Verify loaded
    assert:
      - type: element-exists
        selector: "#main-content"
```

Full schema reference: [workflow-format.md](references/workflow-format.md)

## Step Types

Each step has a `name` and either an `action` (browser command) or `assert` (verification checks).

### Actions → CLI Command Mapping

| Action | CLI Command | Required Fields |
|--------|-------------|-----------------|
| `open` | `actionbook browser open <url>` | `url` |
| `click` | `actionbook browser click "<selector>"` | `selector` |
| `fill` | `actionbook browser fill "<selector>" "value"` | `selector`, `value` |
| `type` | `actionbook browser type "<selector>" "value"` | `selector`, `value` |
| `select` | `actionbook browser select "<selector>" "value"` | `selector`, `value` |
| `hover` | `actionbook browser hover "<selector>"` | `selector` |
| `press` | `actionbook browser press <key>` | `key` |
| `wait` | `actionbook browser wait "<selector>"` | `selector` |
| `wait-fn` | `actionbook browser wait-fn "<expression>"` | `expression` |
| `wait-idle` | `actionbook browser wait-idle` | — | ⚠ Not supported in extension mode |
| `wait-nav` | `actionbook browser wait-nav` | — |
| `snapshot` | `actionbook browser snapshot` | — |
| `screenshot` | `actionbook browser screenshot` | — |
| `text` | `actionbook browser text [selector]` | `selector` (optional) |
| `eval` | `actionbook browser eval "expression"` | `expression` |
| `upload` | `actionbook browser upload "<selector>" "<file-path>"` | `selector`, `file_path` |
| `scroll` | `actionbook browser scroll <direction>` | `direction` (up/down/top/bottom/to) |
| `emulate` | `actionbook browser emulate <device>` | `device` |
| `info` | `actionbook browser info "<selector>"` | `selector` |
| `console` | `actionbook browser console --level error` | — |
| `close` | `actionbook browser close` | — |

### Step Options

```yaml
- name: Accept cookies if present
  action: click
  selector: "[data-testid='cookie-accept']"
  on_fail: continue     # skip | abort (default) | continue
  retry: 1              # override retry count
  timeout: 5000         # step-level timeout override
  condition: element-exists "[data-testid='cookie-banner']"
```

## Assertion Types

Steps can include `assert` blocks to verify expected outcomes. Common types listed below; see [assertion-types.md](references/assertion-types.md) for the complete reference.

| Type | Description | CLI Mapping |
|------|-------------|-------------|
| `text-contains` | Element text contains string | `browser text "<selector>"` + string check |
| `text-equals` | Element text exactly matches | `browser text "<selector>"` + exact match |
| `text-matches` | Text matches regex pattern | `browser text "<selector>"` + regex |
| `url-contains` | Current URL contains string | `browser eval "location.href"` |
| `url-equals` | Current URL exactly matches | `browser eval "location.href"` |
| `element-exists` | Element present in DOM | `browser wait "<selector>" --timeout 5000` |
| `element-not-exists` | Element NOT present | `browser eval "!document.querySelector(...)"` |
| `element-visible` | Element is visible | `browser eval` visibility check |
| `element-hidden` | Element hidden or absent | Inverse of `element-visible` |
| `element-count` | Element count matches condition | `browser eval "querySelectorAll(...).length"` |
| `attribute-equals` | Element attribute matches | `browser eval "getAttribute(...)"` |
| `attribute-contains` | Attribute contains substring | `browser eval "getAttribute(...)"` |
| `page-title-contains` | Page title contains string | `browser eval "document.title"` |
| `eval-truthy` | JS expression evaluates truthy | `browser eval "<expression>"` |
| `console-no-errors` | No JS errors in console | `browser console --level error` |
| `network-no-failures` | No HTTP 4xx/5xx errors | Network monitoring via CDP |
| `screenshot-match` | Visual regression comparison | `browser screenshot` + pixel diff |
| `performance-under` | Performance metric under threshold | `browser eval` performance timing |

### Assertion Examples

```yaml
# Text assertions
- name: Verify welcome message
  assert:
    - type: text-contains
      selector: "[data-testid='welcome']"
      value: "Welcome back"

# URL assertions
- name: Verify redirect
  assert:
    - type: url-contains
      value: "/dashboard"

# Element count with operator
- name: Verify search results
  assert:
    - type: element-count
      selector: ".search-result"
      operator: ">="
      value: 5

# JS evaluation
- name: Verify cart state
  assert:
    - type: eval-truthy
      expression: "JSON.parse(localStorage.getItem('cart')).items.length > 0"
```

## Execution Flow

### Step 0: Pre-flight Checks

Before running any test, verify the environment is ready:

```bash
# 1. Check browser connection
actionbook browser status
# If no browser → open one with setup flags

# 2. Verify target site is reachable (fast check, no rendering)
actionbook browser fetch <url> --format text --timeout 10000 --lite
# If fails → report site unreachable, skip all tests for this domain

# 3. Start console error monitoring
actionbook browser console --level error --duration 0 &
# Capture JS errors throughout the test session
```

Pre-flight failures should be reported clearly — distinguish "test failed" from "environment broken".

### Step 1: Discover

Parse YAML workflow files from the `tests/` directory. Filter by `--filter` flag (matches tags or name).

```bash
# Run all tests
/actionbook-web-test run tests/

# Run smoke tests only
/actionbook-web-test run tests/smoke/

# Filter by tag
/actionbook-web-test run tests/ --filter critical
```

### Step 2: Setup

For each workflow:
1. Pre-fetch selectors: `actionbook search` + `actionbook get "<action-id>"` for each entry in `actions`
2. Resolve template variables (`{{env.VAR}}`, `{{timestamp}}`, etc.)
3. Restore auth state if `setup.profile` is specified (cookies/storage from previous session)
4. Open browser with configured flags:
   ```bash
   actionbook --auto-dismiss-dialogs --no-animations browser open <url>
   ```
5. If `setup.emulate` is set, apply device emulation:
   ```bash
   actionbook browser emulate iphone-14
   ```

### Step 3: Execute

For each step in order:
1. Check `condition` (if present) — skip step if condition is false
2. **Pre-check element** (for interaction steps): use `info` to verify element state
   ```bash
   actionbook browser info "<selector>"
   # Returns: bounding box, visibility, enabled state, attributes
   ```
3. Translate action to `actionbook browser` CLI command
4. Execute the command
5. If step has `assert` block: run each assertion check
6. On **PASS**: log success, continue to next step
7. On **FAIL**: enter recovery (Step 4) or handle per `on_fail` setting
8. **After the last step of each test** (regardless of PASS/FAIL/SKIP): capture a screenshot of the current page state. This screenshot will be embedded in the report under that test's section.
   ```bash
   # Auto-capture at end of each test — save to a per-test temp file
   actionbook browser screenshot /tmp/test-<test-name>-final.png
   base64 -i /tmp/test-<test-name>-final.png | tr -d '\n' > /tmp/test-<test-name>-final-b64.txt
   ```

**Smart Waits**: Always prefer `wait-fn` over `eval "setTimeout"`:

```bash
# BAD: blind delay
actionbook browser eval "new Promise(r => setTimeout(r, 800))"

# GOOD: wait for condition
actionbook browser wait-fn "document.querySelector('#sidebar').offsetWidth < 100" --timeout 5000

# GOOD: wait for element state change
actionbook browser wait-fn "document.querySelector('.loading').style.display === 'none'" --timeout 10000

# GOOD: wait for URL change after click
actionbook browser wait-fn "window.location.href.includes('/dashboard')" --timeout 10000
```

### Step 4: Recover

| Error | Recovery Strategy | Retries |
|-------|-------------------|---------|
| Selector not found | `snapshot` → find equivalent selector → retry step | 1 |
| Navigation timeout | `wait "<selector>" --timeout 15000` → retry (use `wait` instead of `wait-idle` in extension mode) | 1 |
| Element not clickable | `scroll to "<selector>"` + `wait` → retry | 1 |
| Element not visible | `info "<selector>"` to check state → scroll/wait → retry | 1 |
| Login wall detected | Check `cookies list` → if no auth, pause for user to log in, resume | 0 (manual) |
| Anti-bot / CAPTCHA | Add `--stealth`, `fingerprint rotate` → retry | 1 |
| Assertion failure | Screenshot + log actual vs expected (genuine failure) | 0 |
| Browser crash | Re-open browser, restart from failed step | 1 |

**Selector recovery detail:**

When a selector from Actionbook or the workflow YAML fails at runtime:
```bash
# 1. Snapshot the live page
actionbook browser snapshot --interactive --compact --max-tokens 800

# 2. Find the equivalent element in the snapshot output
# 3. Use the new selector to retry the failed step
```

### Step 5: Teardown

```bash
# Capture any accumulated JS errors before closing
actionbook browser console --level error

# Close browser
actionbook browser close
```

Always close the browser, even on test failure.

### Step 6: Report

Generate test results in the requested format. See [Report Generation](#report-generation) for details.

## Selector Strategy

Selectors come from three sources: **Actionbook API** (verified, health-scored), **workflow YAML** (static), and **live snapshot** (runtime fallback).

| Priority | Source | When to Use |
|----------|--------|-------------|
| 1 | `actionbook search` + `get` | Build phase — discover and pre-fill selectors for target pages |
| 2 | `data-testid` / `aria-label` | Stable attributes written directly in workflow YAML |
| 3 | CSS selector | Specified directly in workflow steps |
| 4 | `actionbook browser snapshot` | Runtime fallback when all above selectors fail |

### Test Construction Flow

Tests are **built using Actionbook selectors**, not hand-written:

```bash
# 1. Search for the target page's action
actionbook search "reddit homepage sidebar navigation search" --domain reddit.com

# 2. Get the full page structure with verified selectors
actionbook get "reddit.com:/search/:default"
# → Returns page structure with inline CSS selectors:
#   Sidebar container: #left-sidebar-container
#   Collapse button: #flex-nav-collapse-button
#   Feed sort links: a[href*='/hot/?feed=home']
#   Search results: main
#   ...

# 3. Use these selectors to write the YAML test
```

This means **you don't need to manually inspect the page** — Actionbook provides verified, health-scored selectors that are regularly maintained.

## Advanced Selectors

### Shadow DOM

Standard CSS selectors cannot pierce Shadow DOM boundaries. To interact with elements inside a Shadow DOM, use `actionbook browser eval` to traverse the shadow root:

```bash
# Click a button inside a Shadow DOM
actionbook browser eval "document.querySelector('host-element').shadowRoot.querySelector('button.inner').click()"

# Read text from inside a Shadow DOM
actionbook browser eval "document.querySelector('host-element').shadowRoot.querySelector('.label').textContent"
```

In a workflow step:
```yaml
- name: Click shadow DOM button
  action: eval
  expression: "document.querySelector('host-element').shadowRoot.querySelector('button.submit').click()"
```

For deeply nested shadow roots, chain `.shadowRoot.querySelector(...)` calls.

### Extension Mode Constraints

When running via the browser extension backend (as opposed to a full Playwright/CDP connection), certain features are unavailable or behave differently:

| Constraint | Workaround |
|-----------|------------|
| `wait-idle` not supported | Use `wait "<selector>"` with timeout, or `wait-fn "<condition>"` for state changes. Only use `eval "new Promise(r => setTimeout(r, N))"` as last resort for pure animation delays. |
| `fill`/`type` incompatible with Web Components | Web Components with Shadow DOM inputs (e.g., Reddit's `faceplate-search-input`) cannot be filled via `fill`/`type`. Use `eval` to set `.value` directly, or navigate to the target URL with query parameters |
| Shadow DOM selector piercing | Standard CSS selectors cannot reach inside Shadow DOM. Use `eval` with `.shadowRoot.querySelector()` |

**Example — Web Component input workaround:**
```yaml
# Instead of: fill "input[name='q']" "search term"
# Navigate directly to the search results URL:
- name: Navigate to search results
  action: open
  url: "https://www.reddit.com/search/?q=actionbook"
```

### Iframes

Elements inside iframes exist in a separate document context. Use `actionbook browser eval` to access iframe content:

```bash
# Click a button inside an iframe
actionbook browser eval "document.querySelector('iframe#payment').contentDocument.querySelector('button.pay').click()"

# Read text from inside an iframe
actionbook browser eval "document.querySelector('iframe#payment').contentDocument.querySelector('.total').textContent"
```

In a workflow step:
```yaml
- name: Fill iframe form field
  action: eval
  expression: "document.querySelector('iframe#payment').contentDocument.querySelector('#card-number').value = '4111111111111111'"
```

> **Note:** `contentDocument` only works for same-origin iframes. Cross-origin iframes cannot be accessed via JavaScript due to browser security policies.

### Multi-Tab Handling

When an action (e.g., clicking a link with `target="_blank"`) opens a new tab, the browser context remains on the original tab. Use these commands to manage multiple tabs:

```bash
# List all open tabs
actionbook browser pages

# Switch to a specific tab by page ID
actionbook browser switch <page_id>
```

In a workflow:
```yaml
- name: Click link that opens new tab
  action: click
  selector: "a[target='_blank']"

- name: Switch to new tab
  action: eval
  expression: "/* use 'actionbook browser pages' to find the new tab's page_id, then 'actionbook browser switch <page_id>' */"
```

> **Note:** After `actionbook browser pages`, identify the new tab by its URL or title, then use `actionbook browser switch <page_id>` to move context to that tab. All subsequent commands will execute against the switched tab.

## Result Reporting

### Console Output (default)

```
actionbook-web-test results
========================
  PASS  google-search-smoke    (6 steps, 3.2s)
  FAIL  app-login-flow         (step 4: "Click submit" - selector not found)
  SKIP  checkout-e2e           (requires login)

Results: 1 passed, 1 failed, 1 skipped (3 total)
Duration: 12.4s
```

### JSON Output (--json)

```bash
/actionbook-web-test run tests/ --json --output results.json
```

```json
{
  "timestamp": "2026-03-13T10:00:00Z",
  "results": [
    {
      "name": "google-search-smoke",
      "status": "passed",
      "steps": { "total": 6, "passed": 6, "failed": 0 },
      "assertions": { "total": 3, "passed": 3, "failed": 0 },
      "duration": 3200
    },
    {
      "name": "app-login-flow",
      "status": "failed",
      "steps": { "total": 7, "passed": 3, "failed": 1, "skipped": 3 },
      "failedStep": {
        "name": "Click submit",
        "error": "Selector not found: button[type='submit']",
        "screenshot": "screenshots/app-login-flow-step4.png"
      },
      "duration": 8100
    }
  ],
  "summary": { "passed": 1, "failed": 1, "skipped": 1, "total": 3, "duration": 12400 }
}
```

## Report Generation

After test execution, generate a visual HTML report using **json-ui**. The agent constructs a json-ui JSON document from the test results, then renders it to HTML.

### How It Works

1. **Collect results** — Track each step's status, duration, error, and screenshot file path during execution
2. **Encode screenshots** — Convert all captured PNG screenshots to base64 (store in temp files)
3. **Build json-ui JSON** — Use a Python/Node script to construct the `Report` node tree, embedding base64 screenshots as `Image` components in each section
4. **Render to HTML** — `npx @actionbookdev/json-ui render report.json -o report.html`
5. **Open in browser** — Show the report to the user

### json-ui Report Template

The agent should generate a JSON document following this structure:

```json
{
  "type": "Report",
  "props": { "title": "Actionbook Test Report", "theme": "auto" },
  "children": [
    {
      "type": "BrandHeader",
      "props": {
        "badge": "Actionbook Test",
        "poweredBy": "actionbook-web-test",
        "showBadge": true
      }
    },
    {
      "type": "Section",
      "props": { "title": "Summary", "icon": "chart" },
      "children": [
        {
          "type": "MetricsGrid",
          "props": {
            "cols": 5,
            "metrics": [
              { "label": "Total", "value": "3", "icon": "list" },
              { "label": "Passed", "value": "1", "trend": "up", "icon": "check" },
              { "label": "Failed", "value": "1", "trend": "down", "icon": "warning" },
              { "label": "Skipped", "value": "1", "icon": "skip" },
              { "label": "Duration", "value": "12.4s", "icon": "clock" }
            ]
          }
        }
      ]
    },
    {
      "type": "Section",
      "props": { "title": "Test Results", "icon": "code" },
      "children": [
        {
          "type": "Table",
          "props": {
            "columns": [
              { "key": "status", "label": "Status" },
              { "key": "name", "label": "Test Name" },
              { "key": "steps", "label": "Steps" },
              { "key": "assertions", "label": "Assertions" },
              { "key": "duration", "label": "Duration" }
            ],
            "rows": [
              {
                "status": "PASS",
                "name": "google-search-smoke",
                "steps": "6/6",
                "assertions": "3/3",
                "duration": "3.2s"
              },
              {
                "status": "FAIL",
                "name": "app-login-flow",
                "steps": "3/7",
                "assertions": "1/2",
                "duration": "8.1s"
              }
            ],
            "striped": true
          }
        }
      ]
    },
    {
      "type": "Section",
      "props": { "title": "google-search-smoke — Step Details", "icon": "check", "collapsible": true },
      "children": [
        {
          "type": "ContributionList",
          "props": {
            "numbered": true,
            "items": [
              { "title": "Open Google", "badge": "PASS", "description": "`browser open https://google.com` (0.5s)" },
              { "title": "Verify search box", "badge": "PASS", "description": "`browser wait \"input[name='q']\"` (0.3s)" },
              { "title": "Fill search query", "badge": "PASS", "description": "`browser fill \"input[name='q']\" \"actionbook\"` (0.2s)" },
              { "title": "Submit search", "badge": "PASS", "description": "`browser press Enter` (0.1s)" },
              { "title": "Verify results loaded", "badge": "PASS", "description": "`browser wait \"#search\"` (1.5s)" },
              { "title": "Verify result count", "badge": "PASS", "description": "assert element-count >= 5 (0.6s)" }
            ]
          }
        },
        {
          "type": "Image",
          "props": {
            "src": "data:image/png;base64,...",
            "alt": "google-search-smoke — final state",
            "caption": "Page state after test completed (PASS)"
          }
        }
      ]
    },
    {
      "type": "Section",
      "props": { "title": "app-login-flow — Step Details", "icon": "warning", "collapsible": true },
      "children": [
        {
          "type": "Callout",
          "props": {
            "type": "important",
            "title": "Step 4: Click submit",
            "content": "Selector not found: `button[type='submit']`\n\nThe submit button was not found on the page. This may indicate a UI change or the element has not loaded."
          }
        },
        {
          "type": "ContributionList",
          "props": {
            "numbered": true,
            "items": [
              { "title": "Open login page", "badge": "PASS", "description": "`browser open https://app.example.com/login` (0.8s)" },
              { "title": "Fill username", "badge": "PASS", "description": "`browser fill \"#email\" \"test@example.com\"` (0.2s)" },
              { "title": "Fill password", "badge": "PASS", "description": "`browser fill \"#password\" \"***\"` (0.1s)" },
              { "title": "Click submit", "badge": "FAIL", "description": "`browser click \"button[type='submit']\"` — Selector not found" },
              { "title": "Verify redirect to dashboard", "badge": "SKIP", "description": "Skipped due to previous failure" },
              { "title": "Check welcome message", "badge": "SKIP", "description": "Skipped due to previous failure" },
              { "title": "Close browser", "badge": "SKIP", "description": "Skipped due to previous failure" }
            ]
          }
        },
        {
          "type": "Image",
          "props": {
            "src": "data:image/png;base64,...",
            "alt": "app-login-flow — failure state",
            "caption": "Page state at point of failure (step 4: Click submit)"
          }
        }
      ]
    },
    {
      "type": "BrandFooter",
      "props": {
        "timestamp": "2026-03-13T10:00:12Z",
        "attribution": "Generated by actionbook-web-test"
      }
    }
  ]
}
```

### json-ui Component Usage Guide

| Test Report Section | json-ui Component | Purpose |
|--------------------|--------------------|---------|
| Header | `BrandHeader` | Report title, badge, branding |
| Summary metrics | `MetricsGrid` | Pass/fail/skip counts, total duration |
| Test list | `Table` | Per-test status, step counts, duration |
| Failure details | `Callout` (type: `important`) | Error message, selector, expected vs actual |
| Per-test screenshot | `Image` | Screenshot at end of each test (PASS or FAIL), MUST use base64 data URL. Placed after `ContributionList` in each test's collapsible section |
| Failure callout | `Callout` (type: `important`) + `Image` | For failed tests: error callout before the step list, screenshot shows failure state |
| Step-by-step log | `ContributionList` | Ordered steps with pass/fail badges |
| Console errors | `Callout` (type: `warning`) | JS errors captured during test |
| Environment info | `DefinitionList` | Browser version, viewport, URL, profile |
| Footer | `BrandFooter` | Timestamp, attribution |

### Rendering the Report

The json-ui package is `@actionbookdev/json-ui` on npm. Use `npx` to run it without global install:

```bash
# 1. Agent writes test results to a JSON file (test-report.json)
# 2. Render to HTML and open in browser:
npx @actionbookdev/json-ui render test-report.json -o test-report.html

# Don't auto-open browser:
npx @actionbookdev/json-ui render test-report.json -o test-report.html --no-open

# Pipe from stdin:
cat test-report.json | npx @actionbookdev/json-ui render - -o test-report.html
```

**IMPORTANT**: Always write the JSON to a file first, then render with `npx @actionbookdev/json-ui`. Do NOT attempt to generate raw HTML directly — json-ui handles all styling, theming, dark mode, and responsive layout.

### Embedding Screenshots in Reports

Screenshots MUST be embedded as base64 data URLs using `Image` components. Local file paths (`file://`) do NOT work — browsers block loading local files from HTML for security reasons.

**Capture and encode workflow:**

```bash
# 1. Capture screenshot to a temp file
actionbook browser screenshot /tmp/step-screenshot.png

# 2. Encode to base64 (store in a temp file for later assembly)
base64 -i /tmp/step-screenshot.png | tr -d '\n' > /tmp/step-screenshot-b64.txt
```

**Embed in json-ui JSON using `Image` component:**

```json
{
  "type": "Image",
  "props": {
    "src": "data:image/png;base64,<base64-encoded-content>",
    "alt": "Step description",
    "caption": "Screenshot at this step"
  }
}
```

**IMPORTANT**: Every screenshot step should produce an `Image` node in the report JSON. Place the `Image` node inside the corresponding section, after the `ContributionList` of step details. Use a Python/Node script to assemble the final JSON from base64 files — do NOT attempt to inline large base64 strings manually.

**Report assembly pattern (recommended):**

```bash
# Use a Python script to build the report JSON with embedded base64:
python3 << 'PYEOF'
import json

# Load base64 data from temp files
with open("/tmp/step-screenshot-b64.txt") as f:
    b64 = f.read()

image_node = {
    "type": "Image",
    "props": {
        "src": f"data:image/png;base64,{b64}",
        "alt": "Screenshot description",
        "caption": "Caption text"
    }
}

# ... insert into report JSON children ...
PYEOF
```

### Per-Test Detail Sections

Every test (PASS, FAIL, or SKIP) gets its own collapsible `Section` in the report. Each section contains a `ContributionList` of step details, followed by an `Image` with the test's final screenshot embedded as base64:

```json
{
  "type": "Section",
  "props": { "title": "app-login-flow — Step Details", "icon": "code", "collapsible": true },
  "children": [
    {
      "type": "ContributionList",
      "props": {
        "numbered": true,
        "items": [
          { "title": "Open login page", "badge": "PASS", "description": "browser open https://app.example.com/login (0.8s)" },
          { "title": "Fill username", "badge": "PASS", "description": "browser fill \"#email\" \"test@example.com\" (0.2s)" },
          { "title": "Fill password", "badge": "PASS", "description": "browser fill \"#password\" \"***\" (0.1s)" },
          { "title": "Click submit", "badge": "FAIL", "description": "browser click \"button[type='submit']\" — Selector not found" },
          { "title": "Verify redirect to dashboard", "badge": "SKIP", "description": "Skipped due to previous failure" }
        ]
      }
    },
    {
      "type": "Image",
      "props": {
        "src": "data:image/png;base64,iVBORw0KGgo...",
        "alt": "app-login-flow — failure screenshot",
        "caption": "Screenshot at point of failure (step 4)"
      }
    }
  ]
}
```

**Per-test section structure** (applies to ALL tests, not just failures):
1. `Callout` (type: `important`) — **only for failed tests**: error details before the step list
2. `ContributionList` — step-by-step execution log with PASS/FAIL/SKIP badges
3. `Image` — base64-embedded screenshot captured at the end of that test

The section `icon` should reflect the test outcome: `"check"` for PASS, `"warning"` for FAIL, `"skip"` for SKIP.

Full report format reference: [report-format.md](references/report-format.md)

## Running Tests

```bash
# Run all tests in a directory
/actionbook-web-test run tests/

# Run a single test file
/actionbook-web-test run tests/smoke/google-search.yaml

# Filter by tag
/actionbook-web-test run tests/ --filter smoke

# JSON output
/actionbook-web-test run tests/ --json --output results.json

# HTML report
/actionbook-web-test run tests/ --html --output report.html

# Verbose mode (show each CLI command)
/actionbook-web-test run tests/ --verbose
```

## Auth State Management

Tests that require login should persist and reuse authentication state via Actionbook profiles.

### Save Auth State After Login

```bash
# Use a named profile — cookies and storage persist across sessions
actionbook --profile myapp browser open "https://app.example.com/login"

# After login (manual or automated), the profile saves cookies/storage automatically
# Next run with the same profile reuses the session
actionbook --profile myapp browser open "https://app.example.com/dashboard"
```

### In Workflow YAML

```yaml
setup:
  profile: "myapp-test"    # Reuse saved session

steps:
  - name: Verify already logged in
    assert:
      - type: url-contains
        value: "/dashboard"
    on_fail: continue       # If not logged in, proceed to login steps

  - name: Login if needed
    condition: url-not-contains "/dashboard"
    action: open
    url: "https://app.example.com/login"
    # ... login steps follow
```

### Inspect Stored Auth State

```bash
# Check current cookies
actionbook browser cookies list

# Check specific auth token
actionbook browser storage get "auth_token"

# Clear session to test fresh login
actionbook browser cookies clear --domain app.example.com
```

### Login Wall Handling

When a test hits a login/auth wall:
1. **Check cookies** — `actionbook browser cookies list` to see if session expired
2. **Pause automation** — keep the browser session open
3. **Ask the user** to complete login manually in the same browser window
4. After user confirms, **continue** the test from where it paused
5. If the post-login page is different, run `actionbook search` + `actionbook get` for the new page

## Snapshot-First Test Generation

When the user wants to test a page but no YAML test exists, **generate tests from a live page snapshot** instead of writing YAML from scratch.

### Auto-Generation Flow

```bash
# 1. Open the target page
actionbook browser open "https://example.com/pricing"

# 2. Snapshot the interactive elements
actionbook browser snapshot --interactive --compact

# Output:
# e0 [navigation] "Main nav"
#   e1 [link] "Home" href="/"
#   e2 [link] "Pricing" href="/pricing"  [current]
#   e3 [link] "Docs" href="/docs"
# e4 [heading] "Pricing Plans"
# e5 [button] "Start Free Trial"
# e6 [button] "Contact Sales"
# e7 [region] "FAQ"
#   e8 [button] "What's included?"
#   e9 [button] "Can I cancel anytime?"
```

### From Snapshot → YAML Test

Based on the snapshot output, generate a smoke test that verifies:
1. **Page loads**: key elements from snapshot exist
2. **Navigation works**: clickable links/buttons are functional
3. **Content present**: headings and labels match expected text

```yaml
# Auto-generated from snapshot of https://example.com/pricing
name: pricing-page-smoke
description: Verify pricing page loads with key elements and interactions
url: https://example.com/pricing
tags: [smoke, auto-generated]

steps:
  - name: Open pricing page
    action: open
    url: "https://example.com/pricing"

  - name: Verify page heading
    assert:
      - type: text-contains
        selector: "h1, h2, [role='heading']"
        value: "Pricing"

  - name: Verify CTA buttons exist
    assert:
      - type: element-exists
        selector: "button"
      - type: element-count
        selector: "button"
        operator: ">="
        value: 2

  - name: Verify navigation links
    assert:
      - type: element-exists
        selector: "a[href='/']"
      - type: element-exists
        selector: "a[href='/docs']"

  - name: Click FAQ accordion
    action: click
    selector: "[role='button']:has-text('What\\'s included?')"
    on_fail: continue

  - name: Capture final state
    action: screenshot
```

### When to Auto-Generate

- User says "test this page" without providing a YAML file
- User provides a URL and says "create a smoke test"
- A new page is added and needs basic coverage

Always show the generated YAML to the user for review before execution.

## Device Emulation

Test responsive behavior by emulating mobile/tablet devices:

### In Workflow YAML

```yaml
setup:
  emulate: iphone-14         # Use device preset
  # Or custom viewport:
  viewport:
    width: 414
    height: 896

steps:
  - name: Verify mobile menu button visible
    assert:
      - type: element-visible
        selector: "[data-testid='mobile-menu-toggle']"

  - name: Verify desktop nav hidden on mobile
    assert:
      - type: element-hidden
        selector: "nav.desktop-nav"
```

### Available Device Presets

| Preset | Resolution | User Agent |
|--------|-----------|------------|
| `iphone-14` | 390x844 | Mobile Safari |
| `iphone-se` | 375x667 | Mobile Safari |
| `pixel-7` | 412x915 | Mobile Chrome |
| `ipad` | 820x1180 | Tablet Safari |
| `desktop-hd` | 1920x1080 | Desktop Chrome |

### Multi-Device Testing

Run the same test across multiple devices using matrix:

```yaml
matrix:
  - { device: "iphone-14", expect_mobile: true }
  - { device: "ipad", expect_mobile: false }
  - { device: "desktop-hd", expect_mobile: false }

setup:
  emulate: "{{matrix.device}}"

steps:
  - name: Check mobile menu
    condition: eval-truthy "{{matrix.expect_mobile}}"
    assert:
      - type: element-visible
        selector: ".mobile-menu"
```

## Console Error Monitoring

Capture JavaScript errors during test execution to catch runtime issues.

### How to Use

```bash
# Capture errors during a specific duration
actionbook browser console --level error --duration 5000

# Capture all levels
actionbook browser console --level all --duration 3000
```

### In Workflow Steps

```yaml
# At the end of a test, verify no JS errors occurred
- name: Verify no JavaScript errors
  assert:
    - type: console-no-errors
      ignore:
        - "favicon"
        - "analytics\\.google\\.com"
        - "third-party"

# After a specific interaction, check for errors
- name: Click submit and check for errors
  action: click
  selector: "#submit"

- name: Verify no errors after submit
  action: console
  assert:
    - type: console-no-errors
```

### Console Error Patterns

Common patterns to ignore in `console-no-errors`:

```yaml
ignore:
  - "favicon\\.ico"                    # Missing favicon (very common)
  - "analytics|tracking|gtag"          # Analytics scripts
  - "Failed to load resource.*\\.map"  # Source map 404s
  - "ResizeObserver loop"              # Benign browser warning
  - "third-party"                      # Third-party script errors
```

## Security Considerations

> **WARNING: `eval` and `eval-truthy` execute arbitrary JavaScript in the page context.**
>
> - `eval` runs any JS expression inside the browser page — it has full access to the DOM, cookies, localStorage, and any page-level APIs.
> - **Never use `eval` with untrusted or user-supplied input.** A malicious expression can exfiltrate data, modify page state, or perform actions as the logged-in user.
> - Prefer built-in assertion types (`text-contains`, `element-exists`, etc.) over `eval-truthy` whenever possible. Only use `eval-truthy` when no built-in assertion covers the check.
> - In CI, treat workflow YAML files like code — they can execute arbitrary JS via `eval` steps. All `.test.yml` files should go through code review before merging.

### Screenshots and Sensitive Data

Screenshots captured during test execution may contain sensitive information (passwords, tokens, personal data visible on screen).

- Before taking a screenshot of a page with sensitive fields, consider masking password inputs:
  ```yaml
  - name: Mask password field before screenshot
    action: eval
    expression: "document.querySelector('#password').value = '********'"
  - name: Capture state
    action: screenshot
  ```
- In CI, restrict artifact access permissions. Use short `retention-days` for artifacts containing screenshots.
- For test flows that interact with sensitive data, use the `--exclude-screenshots` flag to skip automatic failure screenshots:
  ```bash
  actionbook test run tests/auth/ --exclude-screenshots
  ```

### Log Redaction

When running in verbose mode (`--verbose`), CLI commands are logged including their arguments. For steps that fill password or secret fields, add `sensitive: true` to redact the value in logs:

```yaml
- name: Fill password
  action: fill
  selector: "#password"
  value: "{{env.TEST_PASSWORD}}"
  sensitive: true   # Value will appear as "***" in verbose logs
```

### Selector Trust Model

CSS selectors in workflow YAML files are treated as trusted input (like code). They are passed directly to browser APIs (`querySelector`). If selectors originate from an untrusted source (e.g., user input, external API), validate them before use to prevent injection.

## References

| Reference | Description |
|-----------|-------------|
| [workflow-format.md](references/workflow-format.md) | Complete YAML workflow schema reference |
| [assertion-types.md](references/assertion-types.md) | All assertion types with examples |
| [report-format.md](references/report-format.md) | json-ui report template and component mapping |

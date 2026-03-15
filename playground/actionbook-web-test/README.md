# actionbook-web-test

A Claude Code Skill for declarative browser web testing. Define tests as YAML workflows, execute them via Actionbook CLI with AI-native selector recovery, and generate visual HTML reports via json-ui.

## What This Is

This is **not** a standalone test framework — it's a set of instructions (SKILL.md) that teaches Claude Code how to:

1. Parse YAML test workflows
2. Translate each step into `actionbook browser` CLI commands
3. Recover from selector failures using live page snapshots
4. Generate visual test reports via `@actionbookdev/json-ui`

## Prerequisites

- [Actionbook CLI](https://github.com/actionbook/actionbook) installed and authenticated
- A browser session available (extension mode or headless)
- `@actionbookdev/json-ui` for report rendering (optional)

## Quick Start

### 1. Write a test YAML

```yaml
name: reddit-ui-smoke
description: Verify Reddit sidebar toggle and search
url: https://www.reddit.com
tags: [smoke, reddit]
timeout: 30000

setup:
  auto_dismiss_dialogs: true
  no_animations: true

steps:
  - name: Open Reddit
    action: open
    url: "https://www.reddit.com"

  - name: Wait for sidebar
    action: wait
    selector: "#left-sidebar-container"
    timeout: 10000

  - name: Click collapse button
    action: click
    selector: "#flex-nav-collapse-button"

  - name: Wait for collapse
    action: wait-fn
    expression: "document.querySelector('#left-sidebar-container').offsetWidth < 100"
    timeout: 5000

  - name: Verify sidebar collapsed
    assert:
      - type: eval-truthy
        expression: "document.querySelector('#left-sidebar-container').offsetWidth < 100"
```

See [tests/reddit-ui-smoke.yaml](tests/reddit-ui-smoke.yaml) for a complete example.

### 2. Run the test via Claude Code

Ask Claude Code to run the test:

```
Test reddit-ui-smoke.yaml against the live site
```

Claude Code reads the YAML, executes each step as `actionbook browser` CLI commands, and reports results.

### 3. Generate a visual report

After test execution, use `generate-report.mjs` to convert test result data into a json-ui report.

#### Step 1: Prepare test result data

Create a JSON file describing environment and test results:

```json
{
  "environment": {
    "timestamp": "2026-03-13T15:30:00Z",
    "browser": "Chromium 125.0 (extension mode)",
    "viewport": "1280x720",
    "profile": "default",
    "target": "https://chatgpt.com"
  },
  "tests": [
    {
      "name": "chatgpt-send-message",
      "status": "passed",
      "duration": 5200,
      "tags": ["smoke", "chatgpt"],
      "summary": "Verified sending a message and receiving a response.",
      "steps": [
        { "name": "Open ChatGPT", "status": "passed", "command": "browser open https://chatgpt.com", "duration": 1200 },
        { "name": "Type message", "status": "passed", "command": "browser fill \"#prompt-textarea\" \"hello\"", "duration": 300 },
        { "name": "Click send", "status": "passed", "command": "browser click \"button[data-testid='send-button']\"", "duration": 200 },
        { "name": "Wait for response", "status": "passed", "command": "browser wait \"[data-message-author-role='assistant']\"", "duration": 3000 },
        { "name": "Verify response exists", "status": "passed", "command": null, "duration": 100, "assertion": true }
      ],
      "screenshot": {
        "path": "screenshots/chatgpt-final.png",
        "alt": "ChatGPT response",
        "caption": "Test completion screenshot"
      }
    }
  ]
}
```

> You can generate an example data file with `node tests/generate-report.mjs --example`

#### Step 2: Generate report JSON

```bash
node tests/generate-report.mjs tests/my-results.json -o tests/report.json
```

The script reads the data, computes summary metrics, embeds screenshots as base64, and outputs the json-ui report structure.

#### Step 3: Render to HTML

```bash
npx @actionbookdev/json-ui render tests/report.json -o report.html
```

#### Data format reference

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `environment.target` | string | yes | The URL being tested |
| `environment.timestamp` | string | no | ISO 8601, defaults to now |
| `environment.browser` | string | no | Browser info |
| `environment.viewport` | string | no | Defaults to `1280x720` |
| `environment.profile` | string | no | Defaults to `default` |
| `tests[].name` | string | yes | Test name |
| `tests[].status` | string | yes | `passed`, `failed`, `skipped` |
| `tests[].duration` | number | yes | Duration in milliseconds |
| `tests[].tags` | string[] | no | Tags for filtering |
| `tests[].summary` | string | no | Human-readable description |
| `tests[].steps[].name` | string | yes | Step name |
| `tests[].steps[].status` | string | yes | `passed`, `failed`, `skipped` |
| `tests[].steps[].command` | string | no | CLI command executed |
| `tests[].steps[].duration` | number | no | Step duration in ms |
| `tests[].steps[].assertion` | boolean | no | `true` if this step is an assertion |
| `tests[].screenshot.path` | string | no | Relative path to screenshot file |
| `tests[].screenshot.alt` | string | no | Alt text |
| `tests[].screenshot.caption` | string | no | Caption |

The report includes:
- Summary metrics (pass/fail/skip/duration)
- Per-test collapsible sections with step-by-step execution logs
- Completion screenshots for each test
- Sidebar TOC with scroll-spy navigation

## Execution Flow

```
Pre-flight → Discover → Setup → Execute → Recover → Teardown → Report
```

| Phase | What happens |
|-------|-------------|
| **Pre-flight** | `browser status` to verify connection, `browser fetch --lite` to check site reachability |
| **Discover** | Parse YAML files, filter by tags |
| **Setup** | Pre-fetch selectors via `actionbook get`, restore auth state, open browser |
| **Execute** | Run each step as CLI commands, use `wait-fn` for smart waits, `info` for element pre-checks |
| **Recover** | On selector failure: `snapshot --interactive` → find equivalent → retry |
| **Teardown** | Capture console errors, `browser close` |
| **Report** | Build json-ui JSON → render HTML report |

## Key Capabilities

| Capability | How |
|-----------|-----|
| Smart waits | `wait-fn "<condition>"` instead of blind `setTimeout` |
| AI selector recovery | `snapshot --interactive` fallback when selectors fail |
| Auth state | `--profile` to persist cookies/storage across sessions |
| Console monitoring | `browser console --level error` to catch JS errors |
| Device emulation | `browser emulate iphone-14` for responsive testing |
| Snapshot-first generation | Auto-generate YAML tests from live page snapshots |
| Visual reports | json-ui HTML with sidebar, collapsible sections, embedded screenshots |

## Project Structure

```
actionbook-web-test/
├── README.md                        # This file
├── SKILL.md                         # Full skill definition for Claude Code
├── references/
│   ├── workflow-format.md           # Complete YAML schema reference
│   ├── assertion-types.md           # All assertion types with examples
│   └── report-format.md            # json-ui report template and components
└── tests/
    ├── reddit-ui-smoke.yaml         # Example test workflow
    └── generate-report.mjs          # Report generation script
```

## References

- [SKILL.md](SKILL.md) — Full skill definition (execution flow, recovery, all CLI mappings)
- [Workflow Format](references/workflow-format.md) — Complete YAML schema
- [Assertion Types](references/assertion-types.md) — All assertion types with examples
- [Report Format](references/report-format.md) — json-ui report template and component mapping

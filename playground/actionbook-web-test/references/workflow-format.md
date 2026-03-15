# Workflow YAML Format Reference

Complete schema reference for actionbook-web-test workflow files (`.test.yml`).

## Schema Overview

A workflow file defines a declarative browser test — a sequence of steps executed against a live website via the Actionbook CLI. Every workflow is a single YAML document with the following top-level fields:

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `name` | string | yes | — | Human-readable test name. Must be unique within a directory. |
| `description` | string | yes | — | What this test validates. Shown in reports. |
| `url` | string | no | — | Starting URL. Opened before the first step. |
| `tags` | string[] | no | `[]` | Arbitrary labels for filtering (`smoke`, `auth`, `regression`). |
| `timeout` | number | no | `30000` | Global step timeout in milliseconds. Individual steps can override. |
| `actions` | string[] | no | `[]` | Actionbook action IDs to pre-fetch verified selectors before execution. Use `actionbook search` to discover IDs, then `actionbook get` to retrieve page structure with selectors. |
| `env` | object | no | `{}` | Key-value pairs injected as template variables. Values support `{{template}}` syntax and `$ENV_VAR` shell expansion. |
| `setup` | object | no | see below | Browser launch configuration. |
| `matrix` | object[] | no | — | Parameterized test data. The workflow runs once per matrix entry. |
| `steps` | Step[] | yes | — | Ordered list of browser actions and assertions. |

### Minimal Example

```yaml
name: Google search smoke test
description: Verify Google search returns results for a known query
url: https://www.google.com
tags: [smoke]

steps:
  - name: Search for actionbook
    action: fill
    selector: "textarea[name='q']"
    value: "actionbook browser automation"

  - name: Submit search
    action: press
    key: Enter

  - name: Wait for results
    action: wait
    selector: "#search"

  - name: Verify results loaded
    action: assert
    assert:
      - type: element-exists
        selector: "#search .g"
      - type: text-contains
        selector: "#search"
        value: "actionbook"
```

---

## Step Schema

Each entry in `steps` is an object describing a single browser interaction or assertion.

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `name` | string | yes | — | Human-readable step description. Appears in test output. |
| `action` | string | yes | — | Action type (see Action Types below). |
| `selector` | string | conditional | — | CSS selector targeting the element. Required for element-interaction actions. |
| `value` | string | conditional | — | Input value for `fill`, `type`, and `select` actions. |
| `url` | string | conditional | — | Target URL for `open` and `goto` actions. |
| `key` | string | conditional | — | Key name for `press` action (e.g. `Enter`, `Tab`, `Escape`). |
| `expression` | string | conditional | — | JavaScript expression for `eval` action. |
| `timeout` | number | no | inherits global | Step-level timeout override in milliseconds. |
| `assert` | Assertion[] | no | `[]` | Assertions to check after this step completes. |
| `on_fail` | string | no | `"abort"` | Failure behavior: `"abort"`, `"skip"`, or `"continue"`. |
| `retry` | number | no | `0` | Number of times to retry this step on failure before applying `on_fail`. |
| `condition` | string | no | — | Conditional expression. Step is skipped if condition evaluates to false. |
| `wait_after` | number | no | `0` | Milliseconds to wait after step completes (debounce for animations, network). |
| `sensitive` | boolean | no | `false` | When `true`, the step's `value` is redacted in verbose logs. Use for password/secret fields. |
| `file_path` | string | conditional | — | File path for `upload` action. |

### Action Types

| Action | Required Fields | CLI Command | Description |
|--------|----------------|-------------|-------------|
| `open` | `url` | `actionbook browser open <url>` | Open URL in a new tab |
| `goto` | `url` | `actionbook browser goto <url>` | Navigate current tab to URL |
| `click` | `selector` | `actionbook browser click "<selector>"` | Click an element |
| `fill` | `selector`, `value` | `actionbook browser fill "<selector>" "<value>"` | Clear field, then type text |
| `type` | `selector`, `value` | `actionbook browser type "<selector>" "<value>"` | Append text (no clear) |
| `select` | `selector`, `value` | `actionbook browser select "<selector>" "<value>"` | Select dropdown option by value |
| `hover` | `selector` | `actionbook browser hover "<selector>"` | Hover over element |
| `focus` | `selector` | `actionbook browser focus "<selector>"` | Focus an element |
| `press` | `key` | `actionbook browser press <key>` | Press a keyboard key |
| `wait` | `selector` | `actionbook browser wait "<selector>"` | Wait for element to appear |
| `wait-fn` | `expression` | `actionbook browser wait-fn "<expression>"` | Wait for JS condition to be truthy |
| `wait-idle` | — | `actionbook browser wait-idle` | Wait for network idle (not supported in extension mode) |
| `wait-nav` | — | `actionbook browser wait-nav` | Wait for navigation to complete |
| `screenshot` | — | `actionbook browser screenshot` | Capture screenshot |
| `text` | `selector` (optional) | `actionbook browser text "<selector>"` | Get element or page text content |
| `eval` | `expression` | `actionbook browser eval "<expression>"` | Execute JavaScript |
| `upload` | `selector`, `file_path` | `actionbook browser upload "<selector>" "<file_path>"` | Upload a file via file input |
| `scroll` | `direction` | `actionbook browser scroll <direction>` | Scroll page (up/down/top/bottom/to) |
| `back` | — | `actionbook browser back` | Navigate back |
| `forward` | — | `actionbook browser forward` | Navigate forward |
| `reload` | — | `actionbook browser reload` | Reload current page |
| `snapshot` | — | `actionbook browser snapshot` | Capture accessibility tree (for fallback selector resolution) |
| `emulate` | `device` | `actionbook browser emulate <device>` | Emulate device (iphone-14, ipad, etc.) |
| `info` | `selector` | `actionbook browser info "<selector>"` | Get element details (box, visibility, attrs) |
| `console` | — | `actionbook browser console --level error` | Capture console messages |
| `assert` | `assert` | — | Pure assertion step (no browser interaction, only checks) |
| `close` | — | `actionbook browser close` | Close the browser session |

---

## Template Variables

String values in `url`, `value`, `expression`, `selector`, and `env` fields support template interpolation using double-brace syntax.

### Built-in Variables

| Variable | Type | Description | Example Output |
|----------|------|-------------|----------------|
| `{{timestamp}}` | number | Current Unix timestamp (seconds) | `1710345600` |
| `{{timestamp_ms}}` | number | Current Unix timestamp (milliseconds) | `1710345600000` |
| `{{date}}` | string | ISO 8601 date string | `2026-03-13T10:30:00Z` |
| `{{date_short}}` | string | Short date (`YYYY-MM-DD`) | `2026-03-13` |
| `{{random}}` | string | Random 8-character hex string | `a3f7b2c1` |
| `{{uuid}}` | string | UUID v4 | `550e8400-e29b-41d4-a716-446655440000` |

### Environment Variables

Variables defined in the `env` block are accessed via `{{env.VAR_NAME}}`:

```yaml
env:
  BASE_URL: https://staging.example.com
  TEST_USER: testuser-{{random}}@example.com
  TEST_PASSWORD: $TEST_PASSWORD  # Resolved from shell environment

steps:
  - name: Open staging site
    action: open
    url: "{{env.BASE_URL}}/login"

  - name: Fill username
    action: fill
    selector: "#email"
    value: "{{env.TEST_USER}}"
```

Shell environment variables are expanded with `$VAR` or `${VAR}` syntax in `env` values only. This allows secrets to be injected from CI without appearing in the YAML file.

### Matrix Variables

When using `matrix`, each key in the matrix entry is available as `{{matrix.KEY}}`:

```yaml
matrix:
  - { query: "typescript", min_results: 5 }
  - { query: "rust lang", min_results: 3 }

steps:
  - name: Search for {{matrix.query}}
    action: fill
    selector: "#search-input"
    value: "{{matrix.query}}"
```

---

## Browser Setup Options

The `setup` block configures how the browser is launched. All fields are optional.

```yaml
setup:
  headless: true                    # Run without visible window (default: false, CI auto-detects)
  stealth: false                    # Enable stealth mode to avoid bot detection (default: false)
  block_images: false               # Block image loading for faster tests (default: false)
  auto_dismiss_dialogs: true        # Auto-dismiss alert/confirm/prompt dialogs (default: true)
  no_animations: true               # Inject CSS to disable transitions/animations (default: true)
  rewrite_urls: false               # Reserved for URL rewriting rules (default: false)
  viewport:                         # Browser viewport dimensions
    width: 1280                     # Default: 1280
    height: 720                     # Default: 720
  user_agent: "custom UA string"    # Override user agent (default: browser default)
  profile: "myapp"                  # Use a named Actionbook browser profile for session persistence
  timeout: 60000                    # Browser launch timeout in ms (default: 30000)
```

**Field details:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `headless` | boolean | `false` (local), auto in CI | No visible browser window. CI environments auto-enable. |
| `stealth` | boolean | `false` | Patches browser fingerprinting signals. Use for sites with bot detection. |
| `block_images` | boolean | `false` | Blocks image requests. Speeds up page loads for non-visual tests. |
| `auto_dismiss_dialogs` | boolean | `true` | Automatically dismisses `alert()`, `confirm()`, `prompt()` dialogs. |
| `no_animations` | boolean | `true` | Injects `* { transition: none !important; animation: none !important; }`. Prevents flaky waits. |
| `rewrite_urls` | boolean | `false` | Reserved for future URL rewriting/proxy support. |
| `viewport` | object | `{width: 1280, height: 720}` | Viewport size. Some assertions (e.g., `element-visible`) depend on viewport. |
| `user_agent` | string | browser default | Custom User-Agent header. |
| `profile` | string | — | Named Actionbook profile. Persists cookies/storage across runs. |
| `timeout` | number | `30000` | Browser launch timeout. Increase for slow CI environments. |

---

## Conditional Steps

Steps can include a `condition` field. The step executes only if the condition evaluates to true. If false, the step is silently skipped (not counted as a failure).

### Condition Syntax

Conditions reuse the assertion type system with a compact string format:

```
<assertion-type> [parameters...]
```

### Examples

```yaml
# Click cookie banner only if it exists
- name: Accept cookies if banner present
  action: click
  selector: "[data-testid='cookie-accept']"
  condition: element-exists "[data-testid='cookie-banner']"
  on_fail: continue

# Skip step if already on the right page
- name: Navigate to dashboard
  action: goto
  url: https://app.example.com/dashboard
  condition: url-not-contains "/dashboard"

# Only fill field if it's visible (some forms progressively disclose fields)
- name: Fill optional company field
  action: fill
  selector: "#company"
  value: "Acme Corp"
  condition: element-visible "#company"
```

### Supported Condition Types

| Condition | Parameters | True when... |
|-----------|------------|------------|
| `element-exists "<selector>"` | CSS selector | Element is in the DOM |
| `element-not-exists "<selector>"` | CSS selector | Element is not in the DOM |
| `element-visible "<selector>"` | CSS selector | Element is visible in viewport |
| `url-contains "<substring>"` | URL substring | Current URL contains substring |
| `url-not-contains "<substring>"` | URL substring | Current URL does not contain substring |
| `eval-truthy "<expression>"` | JS expression | Expression returns truthy value |

---

## Data-Driven Tests (Matrix)

The `matrix` field accepts an array of objects. The entire workflow runs once for each entry, with matrix values available as `{{matrix.KEY}}` template variables.

### Basic Matrix

```yaml
name: Search results validation
description: Verify search returns relevant results for multiple queries
url: https://example.com/search
tags: [smoke, search]

matrix:
  - { query: "typescript", expected_text: "TypeScript" }
  - { query: "rust programming", expected_text: "Rust" }
  - { query: "python tutorial", expected_text: "Python" }

steps:
  - name: Enter search query
    action: fill
    selector: "#search-input"
    value: "{{matrix.query}}"

  - name: Submit search
    action: press
    key: Enter

  - name: Wait for results
    action: wait
    selector: ".search-results"

  - name: Verify results contain expected text
    action: assert
    assert:
      - type: text-contains
        selector: ".search-results"
        value: "{{matrix.expected_text}}"
```

### Matrix with Multiple Assertions

```yaml
matrix:
  - { path: "/pricing", title: "Pricing", has_cta: true }
  - { path: "/about", title: "About Us", has_cta: false }
  - { path: "/contact", title: "Contact", has_cta: true }

steps:
  - name: Navigate to {{matrix.path}}
    action: goto
    url: "https://example.com{{matrix.path}}"

  - name: Verify page title
    action: assert
    assert:
      - type: page-title-contains
        value: "{{matrix.title}}"

  - name: Check CTA button
    action: assert
    condition: eval-truthy "{{matrix.has_cta}}"
    assert:
      - type: element-exists
        selector: ".cta-button"
```

### Test Report Output with Matrix

Each matrix entry produces a separate test result in the report:

```
Search results validation [query=typescript]     PASS  (1.2s)
Search results validation [query=rust programming] PASS  (1.4s)
Search results validation [query=python tutorial] FAIL  (2.1s)
  Step: Verify results contain expected text
  Assertion failed: text-contains — expected ".search-results" to contain "Python"
  Screenshot: .actionbook-web-test/failures/search-results-validation-python-tutorial.png
```

---

## Failure Handling

### on_fail Behavior

| Value | Behavior |
|-------|----------|
| `"abort"` | (default) Stop the entire workflow immediately. Mark test as FAIL. |
| `"skip"` | Skip remaining assertions in this step and move to the next step. Step marked as SKIPPED. |
| `"continue"` | Record the failure but continue executing subsequent steps. Test still marked FAIL at the end. |

### Retry Logic

The `retry` field specifies how many times to retry a failing step before applying `on_fail`. This is useful for steps that may fail due to timing (animations, lazy loading, network).

```yaml
- name: Wait for dynamically loaded content
  action: wait
  selector: ".lazy-content"
  timeout: 5000
  retry: 2          # Try up to 3 times total (1 initial + 2 retries)
  on_fail: continue # If still failing after retries, continue test
```

Between retries, the runner waits 500ms (not configurable). Each retry resets the step timeout.

---

## Complete Workflow Example

```yaml
name: E-commerce checkout flow
description: Verify a user can search for a product, add to cart, and reach checkout
url: https://shop.example.com
tags: [e2e, checkout, critical]
timeout: 15000

env:
  SEARCH_TERM: "wireless headphones"
  TEST_EMAIL: buyer-{{random}}@test.example.com

setup:
  headless: true
  no_animations: true
  viewport: { width: 1440, height: 900 }

steps:
  - name: Accept cookie consent if present
    action: click
    selector: "[data-testid='cookie-accept']"
    condition: element-exists "[data-testid='cookie-banner']"
    on_fail: continue

  - name: Search for product
    action: fill
    selector: "#search-input"
    value: "{{env.SEARCH_TERM}}"

  - name: Submit search
    action: press
    key: Enter

  - name: Wait for results
    action: wait
    selector: ".product-grid"
    timeout: 10000

  - name: Verify search returned results
    action: assert
    assert:
      - type: element-count
        selector: ".product-card"
        operator: ">="
        value: 1
      - type: text-contains
        selector: ".product-grid"
        value: "headphones"

  - name: Click first product
    action: click
    selector: ".product-card:first-child a"

  - name: Wait for product page
    action: wait-nav

  - name: Add to cart
    action: click
    selector: "[data-testid='add-to-cart']"
    retry: 1

  - name: Open cart
    action: click
    selector: "[data-testid='cart-icon']"

  - name: Wait for cart page
    action: wait-nav

  - name: Verify item in cart
    action: assert
    assert:
      - type: element-count
        selector: ".cart-item"
        operator: ">="
        value: 1
      - type: url-contains
        value: "/cart"

  - name: Proceed to checkout
    action: click
    selector: "[data-testid='checkout-btn']"

  - name: Wait for checkout page
    action: wait-nav

  - name: Verify checkout page loaded
    action: assert
    assert:
      - type: url-contains
        value: "/checkout"
      - type: element-exists
        selector: "#email"

  - name: Capture final state
    action: screenshot
```

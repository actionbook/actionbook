# Assertion Types Reference

Complete catalog of assertion types available in actionbook-web-test workflow files. Assertions validate the state of the page after a step executes.

## Assertion Syntax

Assertions appear in the `assert` array of a step:

```yaml
- name: Verify page state
  action: assert
  assert:
    - type: <assertion-type>
      selector: "<css-selector>"     # If the assertion targets an element
      value: "<expected-value>"      # The value to compare against
      operator: ">="               # For numeric comparisons
      pattern: "<regex>"            # For regex assertions
      message: "Custom failure msg" # Optional override for failure output
```

Every assertion has a `type` (required). Other fields depend on the assertion type. The `message` field is always optional and overrides the default failure description.

---

## Text Assertions

### text-contains

Check that an element's text content contains a substring (case-sensitive).

**Parameters:**
| Parameter | Required | Description |
|-----------|----------|-------------|
| `selector` | yes | CSS selector for the target element |
| `value` | yes | Substring to search for |

**CLI mapping:** `actionbook browser text "<selector>"` — runner checks if output contains `value`.

**Example:**
```yaml
- type: text-contains
  selector: ".welcome-banner"
  value: "Hello, John"
```

**Edge cases:**
- Matches against the element's `textContent`, which includes text from child elements.
- Whitespace is normalized (consecutive spaces/newlines collapsed to single space) before comparison.
- Empty `value` always passes — avoid this.

---

### text-equals

Check that an element's text content exactly equals a value (after whitespace normalization).

**Parameters:**
| Parameter | Required | Description |
|-----------|----------|-------------|
| `selector` | yes | CSS selector for the target element |
| `value` | yes | Exact expected text |

**CLI mapping:** `actionbook browser text "<selector>"` — runner compares normalized output to `value`.

**Example:**
```yaml
- type: text-equals
  selector: "h1.page-title"
  value: "Dashboard"
```

**Edge cases:**
- Leading/trailing whitespace is trimmed before comparison.
- Interior whitespace is normalized (multiple spaces become one).
- Does not match against `innerHTML` — only visible text content.

---

### text-matches

Check that an element's text content matches a regular expression.

**Parameters:**
| Parameter | Required | Description |
|-----------|----------|-------------|
| `selector` | yes | CSS selector for the target element |
| `pattern` | yes | Regular expression pattern (without delimiters) |
| `flags` | no | Regex flags string (e.g., `"i"` for case-insensitive, `"gi"` for global + case-insensitive). Default: none (case-sensitive). |

**CLI mapping:** `actionbook browser text "<selector>"` — runner tests output against regex.

**Example:**
```yaml
- type: text-matches
  selector: ".order-id"
  pattern: "^ORD-[A-Z0-9]{8}$"

# Case-insensitive match
- type: text-matches
  selector: ".status-label"
  pattern: "^(active|inactive)$"
  flags: "i"
```

**Edge cases:**
- Pattern is compiled as a full regex, not a glob. Escape special characters.
- Matches against the full normalized text. Use `.*` if the pattern is a substring.
- Matching is case-sensitive by default. Use `flags: "i"` for case-insensitive matching.

---

## URL Assertions

### url-contains

Check that the current page URL contains a substring.

**Parameters:**
| Parameter | Required | Description |
|-----------|----------|-------------|
| `value` | yes | Substring to search for in the URL |

**CLI mapping:** `actionbook browser eval "window.location.href"` — runner checks if output contains `value`.

**Example:**
```yaml
- type: url-contains
  value: "/dashboard"
```

**Edge cases:**
- Matches against the full URL including protocol, domain, path, query string, and hash.
- For query parameter checks, include the `?` or `&` prefix to avoid false matches: `value: "?tab=settings"`.

---

### url-equals

Check that the current page URL exactly matches a value.

**Parameters:**
| Parameter | Required | Description |
|-----------|----------|-------------|
| `value` | yes | Exact expected URL |

**CLI mapping:** `actionbook browser eval "window.location.href"` — runner compares output to `value`.

**Example:**
```yaml
- type: url-equals
  value: "https://app.example.com/dashboard"
```

**Edge cases:**
- Trailing slashes matter. `https://example.com` does not equal `https://example.com/`.
- Query parameters must be in the exact same order.
- Prefer `url-contains` unless exact match is truly required.

---

### url-matches

Check that the current page URL matches a regular expression.

**Parameters:**
| Parameter | Required | Description |
|-----------|----------|-------------|
| `pattern` | yes | Regular expression pattern |
| `flags` | no | Regex flags string (e.g., `"i"` for case-insensitive). Default: none. |

**CLI mapping:** `actionbook browser eval "window.location.href"` — runner tests output against regex.

**Example:**
```yaml
- type: url-matches
  pattern: "^https://app\\.example\\.com/users/[0-9]+/profile$"

# Case-insensitive URL match
- type: url-matches
  pattern: "^https://app\\.example\\.com/dashboard"
  flags: "i"
```

**Edge cases:**
- Remember to escape dots in domain names (`\\.`).
- Query strings contain special regex characters (`?`, `+`). Escape them.

---

## Element Assertions

### element-exists

Check that an element matching the selector exists in the DOM.

**Parameters:**
| Parameter | Required | Description |
|-----------|----------|-------------|
| `selector` | yes | CSS selector |

**CLI mapping:** `actionbook browser eval "document.querySelector('<selector>') !== null"` — runner checks for truthy return.

**Example:**
```yaml
- type: element-exists
  selector: "[data-testid='user-avatar']"
```

**Edge cases:**
- Element may exist in DOM but be hidden (`display: none`). Use `element-visible` if visibility matters.
- Pseudo-elements cannot be selected.

---

### element-not-exists

Check that no element matching the selector exists in the DOM.

**Parameters:**
| Parameter | Required | Description |
|-----------|----------|-------------|
| `selector` | yes | CSS selector |

**CLI mapping:** `actionbook browser eval "document.querySelector('<selector>') === null"` — runner checks for truthy return.

**Example:**
```yaml
- type: element-not-exists
  selector: ".error-banner"
```

**Edge cases:**
- Useful for verifying error states are cleared, modals are dismissed, or elements are removed after an action.
- Beware of timing: element may not have been removed yet. Use `wait_after` on the preceding step or increase `timeout`.

---

### element-visible

Check that an element is visible in the viewport (exists, displayed, and has non-zero dimensions).

**Parameters:**
| Parameter | Required | Description |
|-----------|----------|-------------|
| `selector` | yes | CSS selector |

**CLI mapping:** `actionbook browser eval "(() => { const el = document.querySelector('<selector>'); if (!el) return false; const r = el.getBoundingClientRect(); return r.width > 0 && r.height > 0 && getComputedStyle(el).display !== 'none' && getComputedStyle(el).visibility !== 'hidden'; })()"` — runner checks for truthy return.

**Example:**
```yaml
- type: element-visible
  selector: ".success-toast"
```

**Edge cases:**
- Elements with `opacity: 0` are considered visible (they have dimensions). This matches browser behavior.
- Elements scrolled out of the viewport but in the DOM are considered not visible.
- Viewport size (from `setup.viewport`) affects visibility.

---

### element-hidden

Check that an element either does not exist or is not visible.

**Parameters:**
| Parameter | Required | Description |
|-----------|----------|-------------|
| `selector` | yes | CSS selector |

**CLI mapping:** Inverse of `element-visible` check.

**Example:**
```yaml
- type: element-hidden
  selector: ".loading-spinner"
```

**Edge cases:**
- Passes if element doesn't exist at all (unlike `element-visible` which requires existence).
- Useful for confirming loading states have completed.

---

### element-count

Check the number of elements matching a selector against a numeric condition.

**Parameters:**
| Parameter | Required | Description |
|-----------|----------|-------------|
| `selector` | yes | CSS selector |
| `operator` | yes | Comparison operator: `==`, `!=`, `>`, `>=`, `<`, `<=` |
| `value` | yes | Expected count (number) |

**CLI mapping:** `actionbook browser eval "document.querySelectorAll('<selector>').length"` — runner compares count against `value` using `operator`.

**Example:**
```yaml
- type: element-count
  selector: ".search-result"
  operator: ">="
  value: 5

- type: element-count
  selector: ".error-message"
  operator: "=="
  value: 0
```

**Edge cases:**
- `value` must be a non-negative integer.
- Common mistake: using `value: "5"` (string) vs `value: 5` (number). Both are accepted; the runner coerces to number.

---

## Attribute Assertions

### attribute-equals

Check that an element's attribute has a specific value.

**Parameters:**
| Parameter | Required | Description |
|-----------|----------|-------------|
| `selector` | yes | CSS selector |
| `attribute` | yes | Attribute name (e.g., `href`, `src`, `data-status`) |
| `value` | yes | Expected attribute value |

**CLI mapping:** `actionbook browser eval "document.querySelector('<selector>')?.getAttribute('<attribute>')"` — runner compares output to `value`.

**Example:**
```yaml
- type: attribute-equals
  selector: "a.download-link"
  attribute: "href"
  value: "/files/report.pdf"
```

**Edge cases:**
- Returns `null` if the attribute does not exist — this never equals any string. Use `attribute-contains` with empty string to check existence.
- Boolean attributes (`disabled`, `checked`) return empty string when present. Use `attribute-equals` with `value: ""` or `element-exists "[disabled]"`.

---

### attribute-contains

Check that an element's attribute value contains a substring.

**Parameters:**
| Parameter | Required | Description |
|-----------|----------|-------------|
| `selector` | yes | CSS selector |
| `attribute` | yes | Attribute name |
| `value` | yes | Substring to search for within the attribute value |

**CLI mapping:** `actionbook browser eval "document.querySelector('<selector>')?.getAttribute('<attribute>')"` — runner checks if output contains `value`.

**Example:**
```yaml
- type: attribute-contains
  selector: "img.hero"
  attribute: "src"
  value: "hero-banner"

- type: attribute-contains
  selector: ".status-badge"
  attribute: "class"
  value: "badge-success"
```

**Edge cases:**
- Checking `class` attribute with `attribute-contains` can produce false positives: `"badge-success"` matches `"badge-success-large"`. Prefer a CSS selector like `.badge-success` with `element-exists` when possible.

---

## Page-Level Assertions

### page-title-contains

Check that the page title (`document.title`) contains a substring.

**Parameters:**
| Parameter | Required | Description |
|-----------|----------|-------------|
| `value` | yes | Substring to search for in the title |

**CLI mapping:** `actionbook browser eval "document.title"` — runner checks if output contains `value`.

**Example:**
```yaml
- type: page-title-contains
  value: "Dashboard"
```

---

### page-title-equals

Check that the page title exactly matches a value.

**Parameters:**
| Parameter | Required | Description |
|-----------|----------|-------------|
| `value` | yes | Exact expected title |

**CLI mapping:** `actionbook browser eval "document.title"` — runner compares output to `value`.

**Example:**
```yaml
- type: page-title-equals
  value: "Settings - MyApp"
```

---

## JavaScript Assertions

### eval-truthy

Execute a JavaScript expression and check that it returns a truthy value.

**Parameters:**
| Parameter | Required | Description |
|-----------|----------|-------------|
| `expression` | yes | JavaScript expression to evaluate |

**CLI mapping:** `actionbook browser eval "<expression>"` — runner checks if return value is truthy.

**Example:**
```yaml
- type: eval-truthy
  expression: "document.querySelectorAll('.item').length > 0"

- type: eval-truthy
  expression: "window.localStorage.getItem('auth_token') !== null"

- type: eval-truthy
  expression: "performance.now() < 5000"
```

**Edge cases:**
- The expression runs in the page context, not Node.js. Browser APIs only.
- Return value is serialized via CDP. Complex objects may not serialize cleanly — return primitives.
- `0`, `""`, `null`, `undefined`, `false`, and `NaN` are all falsy. Be precise.
- Expressions that throw are treated as failures, not falsy.

---

## Console & Network Assertions

### console-no-errors

Check that no JavaScript errors were logged to the console during the test (or since the last `console-no-errors` assertion).

**Parameters:**
| Parameter | Required | Description |
|-----------|----------|-------------|
| `ignore` | no | Array of regex patterns. Console errors matching any pattern are ignored. |

**CLI mapping:** Runner monitors console via CDP `Runtime.consoleAPICalled` and `Runtime.exceptionThrown` events.

**Example:**
```yaml
- type: console-no-errors

# With ignored patterns
- type: console-no-errors
  ignore:
    - "Failed to load resource.*favicon"
    - "third-party-script"
```

**Edge cases:**
- Only captures `console.error()` calls and uncaught exceptions, not `console.warn()`.
- Third-party scripts (analytics, ads) often produce errors. Use `ignore` patterns liberally.
- This assertion checks accumulated errors since the last `console-no-errors` check or since test start. Place it at the end of the test for a full-run check.

---

### network-no-failures

Check that no network requests failed (HTTP 4xx/5xx or network errors) during the test.

**Parameters:**
| Parameter | Required | Description |
|-----------|----------|-------------|
| `ignore_status` | no | Array of HTTP status codes to ignore (e.g., `[404]`) |
| `ignore_urls` | no | Array of URL patterns (regex) to ignore |

**CLI mapping:** Runner monitors network via CDP `Network.responseReceived` and `Network.loadingFailed` events.

**Example:**
```yaml
- type: network-no-failures

# With ignored patterns
- type: network-no-failures
  ignore_status: [404]
  ignore_urls:
    - "analytics\\.google\\.com"
    - "sentry\\.io"
```

**Edge cases:**
- CORS-blocked requests show as network failures. Use `ignore_urls` for known cross-origin requests.
- Favicon 404s are extremely common. Consider always ignoring `favicon.ico`.
- WebSocket disconnections are not tracked by this assertion.

---

## Visual Regression

### screenshot-match

Compare a screenshot of the current page (or element) against a stored baseline image.

**Parameters:**
| Parameter | Required | Description |
|-----------|----------|-------------|
| `selector` | no | CSS selector for element screenshot. Omit for full page. |
| `baseline` | yes | Path to baseline image (relative to workflow file) |
| `threshold` | no | Maximum allowed pixel difference ratio (0.0-1.0). Default: `0.01` (1%) |

**CLI mapping:** `actionbook browser screenshot` (or element screenshot via eval) — runner compares against baseline using pixel diff.

**Example:**
```yaml
- type: screenshot-match
  baseline: "baselines/homepage-hero.png"
  threshold: 0.02

- type: screenshot-match
  selector: ".pricing-table"
  baseline: "baselines/pricing-table.png"
  threshold: 0.005
```

**Edge cases:**
- Anti-aliasing differences between OS/browser versions can cause false positives. Set `threshold` accordingly.
- Dynamic content (timestamps, avatars, ads) will always differ. Use element-level screenshots to target stable regions.
- First run with no baseline: the assertion fails and saves the current screenshot as the new baseline. Commit this baseline to version control.
- Viewport size affects screenshots. Always set `setup.viewport` explicitly when using visual regression.

---

## Performance Assertion

### performance-under

Check that a performance metric is under a threshold.

**Parameters:**
| Parameter | Required | Description |
|-----------|----------|-------------|
| `metric` | no | Performance metric name. Default: `"load"`. Options: `"load"`, `"domcontentloaded"`, `"firstpaint"`, `"firstcontentfulpaint"`. |
| `value` | yes | Maximum allowed time in milliseconds |

**CLI mapping:** `actionbook browser eval "performance.timing.loadEventEnd - performance.timing.navigationStart"` (varies by metric) — runner checks if measured time is under `value`.

**Example:**
```yaml
- type: performance-under
  metric: "load"
  value: 3000

- type: performance-under
  metric: "firstcontentfulpaint"
  value: 1500
```

**Edge cases:**
- Performance timing is only available after the page has fully loaded. Place this assertion after a `wait-nav` step.
- SPA navigations don't reset `performance.timing`. This assertion is most reliable for initial page loads.
- CI environments are typically slower than local machines. Set generous thresholds or use `on_fail: continue` to avoid flaky failures.
- `firstpaint` and `firstcontentfulpaint` use the Paint Timing API (`performance.getEntriesByType('paint')`), which may not be available in all browser versions.

---

## Quick Reference Table

| Assertion Type | Requires Selector | Key Parameter | Category |
|---------------|-------------------|---------------|----------|
| `text-contains` | yes | `value` | Text |
| `text-equals` | yes | `value` | Text |
| `text-matches` | yes | `pattern`, `flags` (optional) | Text |
| `url-contains` | no | `value` | URL |
| `url-equals` | no | `value` | URL |
| `url-matches` | no | `pattern`, `flags` (optional) | URL |
| `element-exists` | yes | — | Element |
| `element-not-exists` | yes | — | Element |
| `element-visible` | yes | — | Element |
| `element-hidden` | yes | — | Element |
| `element-count` | yes | `operator`, `value` | Element |
| `attribute-equals` | yes | `attribute`, `value` | Attribute |
| `attribute-contains` | yes | `attribute`, `value` | Attribute |
| `page-title-contains` | no | `value` | Page |
| `page-title-equals` | no | `value` | Page |
| `eval-truthy` | no | `expression` | JavaScript |
| `console-no-errors` | no | `ignore` (optional) | Console |
| `network-no-failures` | no | `ignore_status`, `ignore_urls` (optional) | Network |
| `screenshot-match` | optional | `baseline`, `threshold` | Visual |
| `performance-under` | no | `metric`, `value` | Performance |

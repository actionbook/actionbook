---
name: actionbook-scraper
description: Generate and verify web scraper scripts using Actionbook's verified selectors. Auto-validates generated scripts and fixes errors.
---

# Actionbook Scraper Skill

## ⚠️ CRITICAL: Two-Part Verification

**Every generated script MUST pass BOTH checks:**

| Check | What to Verify | Failure Example |
|-------|----------------|-----------------|
| **Part 1: Script Runs** | No errors, no timeouts | `Selector not found` |
| **Part 2: Data Correct** | Content matches expected | Extracted "Click to expand" instead of name |

```
┌─────────────────────────────────────────────────────┐
│   1. Generate Script                                │
│          ↓                                          │
│   2. Execute Script                                 │
│          ↓                                          │
│   3. Check Part 1: Script runs without errors?      │
│          ↓                                          │
│   4. Check Part 2: Data content is correct?         │
│      - Not empty                                    │
│      - Not placeholder text ("Loading...")          │
│      - Not UI text ("Click to expand")              │
│      - Fields mapped correctly                      │
│          ↓                                          │
│      ┌───┴───┐                                      │
│   BOTH Pass  Either Fails                           │
│      │           │                                  │
│      │           ↓                                  │
│      │       Is it Actionbook data issue?           │
│      │           │                                  │
│      │       ┌───┴───┐                              │
│      │      Yes      No                             │
│      │       │       │                              │
│      │       ↓       ↓                              │
│      │    Log to   Fix script                       │
│      │    .actionbook-issues.log                    │
│      │       │       │                              │
│      │       └───┬───┘                              │
│      │           ↓                                  │
│      │       Retry (max 3x)                         │
│      ↓                                              │
│   Output Script                                     │
└─────────────────────────────────────────────────────┘
```

## Default Output Format

```
/actionbook-scraper:generate <url>
```

**DEFAULT = agent-browser script (bash commands)**

```bash
agent-browser open "https://example.com"
agent-browser scroll down 2000
agent-browser get text ".selector"
agent-browser close
```

## With --standalone Flag

```
/actionbook-scraper:generate <url> --standalone
```

**Output = Playwright JavaScript code**

---

## Verification Requirements

### Two-Part Verification

Every generated script must pass BOTH checks:

| Check | What to Verify | Failure Action |
|-------|---------------|----------------|
| **1. Script Runs** | No errors, no timeouts | Fix syntax/selector errors |
| **2. Data Correct** | Content matches expected fields | Fix extraction logic |

### Part 1: Script Execution Check

- No runtime errors
- No timeout errors
- Browser closes properly

### Part 2: Data Content Check (CRITICAL)

**Verify extracted data matches the expected structure:**

```
Expected: Company name, description, website, year founded
Actual:   "Click to expand", "Loading...", empty strings

→ FAIL: Data content incorrect, need to fix extraction logic
```

**Data validation rules:**

| Rule | Example Failure | Fix |
|------|-----------------|-----|
| Fields not empty | `name: ""` | Check selector targets correct element |
| No placeholder text | `name: "Loading..."` | Add wait for dynamic content |
| No UI text | `name: "Click to expand"` | Extract after expanding, not button text |
| Correct data type | `year: "View Details"` | Wrong selector, fix field mapping |
| Reasonable count | Expected ~100, got 3 | Add scroll/pagination handling |

### For agent-browser Scripts

1. **Execute the generated commands**
2. **Check script runs without errors**
3. **Check data content is correct:**
   - Fields match expected structure
   - Values are actual data, not UI text
   - Count is reasonable
4. **If failed:**
   - Analyze what's wrong (script error vs data error)
   - Fix selector, wait logic, or extraction
   - Re-execute
5. **If success:**
   - Output the verified script
   - Show data preview with field validation

### For Playwright Scripts (--standalone)

1. **Write script to temp file**
2. **Run with `node script.js`**
3. **Check script runs without errors**
4. **Check output data is correct:**
   - JSON structure matches expected fields
   - Values contain actual data
   - Count matches expected range
5. **If failed:**
   - Analyze error type
   - Fix script
   - Re-run
6. **If success:**
   - Output the verified script

## Architecture Overview

```
/generate <url>              → OUTPUT: agent-browser bash commands
/generate <url> --standalone → OUTPUT: Playwright .js file
```

```
┌─────────────────────────────────────────────────────────────┐
│                   /generate <url>                           │
│                                                             │
│   1. Search Actionbook → get selectors                      │
│   2. Generate OUTPUT:                                       │
│                                                             │
│      WITHOUT --standalone    │    WITH --standalone         │
│      ─────────────────────   │    ──────────────────        │
│      agent-browser commands  │    Playwright .js code       │
│                              │                              │
│      ```bash                 │    ```javascript             │
│      agent-browser open ...  │    const { chromium } = ...  │
│      agent-browser get ...   │    await page.goto(...)      │
│      agent-browser close     │    ```                       │
│      ```                     │                              │
└─────────────────────────────────────────────────────────────┘
```

## Tool Priority

| Operation | Primary Tool | Fallback | Notes |
|-----------|-------------|----------|-------|
| Find selectors for URL | `search_actions` | None | Search by domain/keywords |
| Get full selector details | `get_action_by_id` | None | Use action_id from search |
| List available sources | `list_sources` | `search_sources` | Browse all indexed sites |
| Generate agent-browser script | Agent (sonnet) | - | Default mode for /generate |
| Generate Playwright script | Agent (sonnet) | - | Use --standalone flag |
| Structure analysis | Agent (haiku) | - | Parse Actionbook response |
| Request new website | `agent-browser` | Manual | Submit to actionbook.app (ONLY command that executes agent-browser) |

## Workflow Rules

### CRITICAL: Generate → Verify → Fix

**Every generated script MUST be verified by executing it.**

| Step | Action |
|------|--------|
| 1 | Generate script with Actionbook selectors |
| 2 | **Execute script to verify it works** |
| 3 | If failed: analyze error, fix script, go to step 2 |
| 4 | If success: output verified script + data preview |

### Verification Process

**For agent-browser scripts:**
```bash
# Execute each command
agent-browser open "https://example.com"
agent-browser wait --load networkidle
agent-browser get text ".selector"
# Check if data is returned
# If error → fix and retry
agent-browser close
```

**For Playwright scripts (--standalone):**
```bash
# Write to temp file and execute
node /tmp/scraper.js
# Check if output file has data
# If error → fix and retry
```

### Critical Rules

1. **ALWAYS verify generated scripts** - Execute and check BOTH parts
2. **Part 1: Script must run** - No errors, no timeouts
3. **Part 2: Data must be correct** - Not empty, not UI text, fields mapped correctly
4. **Fix errors automatically** - Don't output broken scripts or wrong data
5. **Use Actionbook MCP tools first** - Never guess selectors
6. **Include scroll handling** for lazy-loaded pages
7. **Include expand/collapse logic** for card-based layouts
8. **Always close browser** - Include `agent-browser close`
9. **Retry up to 3 times** - If still failing, report the specific issue

### Common Data Errors to Catch

| Error | Example | Fix |
|-------|---------|-----|
| Extracted button text | `name: "Click to expand"` | Extract content after expanding |
| Extracted placeholder | `desc: "Loading..."` | Add wait for dynamic content |
| Empty fields | `name: ""` | Fix selector |
| Wrong field mapping | `year: "San Francisco"` | Fix selector for each field |
| Too few items | Expected 100, got 3 | Add scroll/pagination |

### Record Actionbook Data Issues

**If Actionbook selectors are wrong or outdated, record to local file:**

```
.actionbook-issues.log
```

**When to record:**
- Selector doesn't exist on page
- Selector returns wrong element
- Page structure has changed
- Missing selectors for key elements

**Log format:**
```
[YYYY-MM-DD HH:MM] URL: {url}
Action ID: {action_id}
Issue Type: {selector_error | outdated | missing}
Details: {description}
Selector: {selector}
Expected: {what it should select}
Actual: {what it actually selects or error}
---
```

### Selector Priority

When Actionbook provides multiple selectors, prefer in this order:
1. `data-testid` - Most stable, designed for automation
2. `aria-label` - Accessibility-based, semantic
3. `css` - Class-based selectors
4. `xpath` - Last resort, most fragile

## Commands

| Command | Description | Agent |
|---------|-------------|-------|
| `/actionbook-scraper:analyze <url>` | Analyze page structure and show available selectors | structure-analyzer |
| `/actionbook-scraper:generate <url>` | Generate agent-browser scraper script | code-generator |
| `/actionbook-scraper:generate <url> --standalone` | Generate Playwright/Puppeteer script | code-generator |
| `/actionbook-scraper:list-sources` | List websites with Actionbook data | - |
| `/actionbook-scraper:request-website <url>` | Request new website to be indexed (uses agent-browser) | website-requester |

## Data Flow

### Analyze Command
```
1. User: /actionbook-scraper:analyze https://example.com/page
2. Extract domain from URL → "example.com"
3. search_actions("example page") → [action_ids]
4. For best match: get_action_by_id(action_id) → full selector data
5. Structure-analyzer agent formats and presents findings
```

### Generate Command (Default: agent-browser script)

```
User: /actionbook-scraper:generate https://example.com/page

Step 1: Search Actionbook
  search_actions("example.com page") → action_ids

Step 2: Get selectors
  get_action_by_id(best_match) → selectors

Step 3: Generate agent-browser script
  ```bash
  agent-browser open "https://example.com/page"
  agent-browser wait --load networkidle
  agent-browser scroll down 2000
  agent-browser get text ".item-container"
  agent-browser close
  ```

Step 4: VERIFY script (REQUIRED)
  Execute the commands and check if data is extracted
  If failed → analyze error → fix script → retry (max 3x)

Step 5: Return verified script + data preview
```

**Example Output:**
````markdown
## Verified Scraper (agent-browser)

**Status**: ✅ Verified (extracted 50 items)

Run these commands to scrape:

```bash
agent-browser open "https://example.com/page"
agent-browser wait --load networkidle
agent-browser scroll down 2000
agent-browser get text ".item-container"
agent-browser close
```

### Data Preview
```json
[
  {"name": "Item 1", "description": "..."},
  {"name": "Item 2", "description": "..."},
  // ... showing first 3 items
]
```
````

### Generate Command (--standalone: Playwright script)

```
User: /actionbook-scraper:generate https://example.com/page --standalone

Step 1: Search Actionbook for selectors
Step 2: Get full selector data
Step 3: Generate Playwright/Puppeteer script
Step 4: VERIFY script (REQUIRED)
  Write to temp file → node /tmp/scraper.js → check output
  If failed → analyze error → fix script → retry (max 3x)
Step 5: Return verified script + data preview
```

**Example Output:**
````markdown
## Verified Scraper (Playwright)

**Status**: ✅ Verified (extracted 50 items)

```javascript
const { chromium } = require('playwright');
// ... generated code with Actionbook selectors
```

Usage:
```bash
npm install playwright
node scraper.js
```

### Data Preview
```json
[
  {"name": "Item 1", "description": "..."},
  // ... first 3 items
]
```
````

### Request Website Command
```
1. User: /actionbook-scraper:request-website https://newsite.com/page
2. Launch website-requester agent (uses agent-browser)
3. Agent workflow:
   a. agent-browser open "https://actionbook.app/request-website"
   b. agent-browser snapshot -i (discover form selectors)
   c. agent-browser type <url-field> "https://newsite.com/page"
   d. agent-browser type <email-field> (optional)
   e. agent-browser type <usecase-field> (optional)
   f. agent-browser click <submit-button>
   g. agent-browser snapshot -i (verify submission)
   h. agent-browser close
4. Output: Confirmation of submission
```

## Selector Data Structure

Actionbook returns selector data in this format:

```json
{
  "url": "https://example.com/page",
  "title": "Page Title",
  "content": "## Selector Reference\n\n| Element | CSS | XPath | Type |\n..."
}
```

### Common Selector Patterns

**Card-based layouts:**
```
Container: .card-list, .grid-container
Card item: .card, .list-item
Card name: .card__title, .card-name
Card description: .card__description
Expand button: .card__expand, button.expand
```

**Detail extraction (dt/dd pattern):**
```javascript
// Common pattern for key-value pairs
const items = container.querySelectorAll('.info-item');
items.forEach(item => {
  const label = item.querySelector('dt').textContent;
  const value = item.querySelector('dd').textContent;
});
```

**Table layouts:**
```
Table: table, .data-table
Header: thead th, .table-header
Row: tbody tr, .table-row
Cell: td, .table-cell
```

## Page Type Detection

| Indicator | Page Type | Template |
|-----------|-----------|----------|
| Scroll to load more | Dynamic/Infinite | playwright-js (with scroll) |
| Click to expand | Card-based | playwright-js (with click) |
| Pagination links | Paginated | playwright-js (with pagination) |
| Static content | Static | puppeteer or playwright |
| SPA framework detected | SPA | playwright-js (network idle) |

## Output Formats

### Analysis Output
```markdown
## Page Analysis: {url}

### Matched Action
- **Action ID**: {action_id}
- **Confidence**: HIGH | MEDIUM | LOW

### Available Selectors

| Element | Selector | Type | Methods |
|---------|----------|------|---------|
| {name} | {selector} | {type} | {methods} |

### Page Structure
- **Type**: {static|dynamic|spa}
- **Data Pattern**: {cards|table|list}
- **Lazy Loading**: {yes|no}
- **Expand/Collapse**: {yes|no}

### Recommendations
- Suggested template: {template}
- Special handling needed: {notes}
```

### Generated Code Output
```markdown
## Generated Scraper

**Target URL**: {url}
**Template**: {template}
**Expected Output**: {description}

### Dependencies
```bash
npm install playwright
```

### Code
```javascript
{generated_code}
```

### Usage
```bash
node scraper.js
```

### Output
Results saved to `{output_file}`
```

## Templates Reference

| Template | Flag | Output | Run With |
|----------|------|--------|----------|
| **agent-browser** | (default) | CLI commands | `agent-browser` CLI |
| playwright-js | --standalone | .js file | `node scraper.js` |
| playwright-python | --standalone --template playwright-python | .py file | `python scraper.py` |
| puppeteer | --standalone --template puppeteer | .js file | `node scraper.js` |

## Error Handling

| Error | Cause | Solution |
|-------|-------|----------|
| No actions found | URL not indexed | Use `/actionbook-scraper:request-website` to request indexing |
| Selectors not working | Page updated | Report to Actionbook, try alternative selectors |
| Timeout | Slow page load | Increase timeout, add retry logic |
| Empty data | Dynamic content | Add scroll/wait handling |
| Form submission failed | Network/page issue | Retry or submit manually at actionbook.app |

## agent-browser Usage

For the `request-website` command, the plugin uses **agent-browser CLI** to automate form submission.

### agent-browser Commands

```bash
# Open a URL
agent-browser open "https://actionbook.app/request-website"

# Get page snapshot (discover selectors)
agent-browser snapshot -i

# Type into form field
agent-browser type "input[name='url']" "https://example.com"

# Click button
agent-browser click "button[type='submit']"

# Close browser (ALWAYS do this)
agent-browser close
```

### Selector Discovery

If form selectors are unknown, use snapshot to discover them:

```bash
agent-browser open "https://actionbook.app/request-website"
agent-browser snapshot -i  # Returns page structure with selectors
```

### Always Close Browser

**Critical**: Always run `agent-browser close` at the end of any agent-browser session, even if errors occur.

## Rate Limiting

- Actionbook MCP: No rate limit for local usage
- Target websites: Respect robots.txt and add delays between requests
- Recommended: 1-2 second delay between page requests

## Examples

### Example 1: Generate agent-browser Script (Default)
```
/actionbook-scraper:generate https://firstround.com/companies

Output: agent-browser commands
```bash
agent-browser open "https://firstround.com/companies"
agent-browser scroll down 2000
agent-browser get text ".company-list-card-small"
agent-browser close
```

User runs these commands to scrape.
```

### Example 2: Generate Playwright Script
```
/actionbook-scraper:generate https://firstround.com/companies --standalone

Output: Playwright JavaScript code
```javascript
const { chromium } = require('playwright');
// ... full script
```

User runs: `node scraper.js`
```

### Example 3: Analyze Page Structure
```
/actionbook-scraper:analyze https://example.com/products

Output: Analysis showing:
- Available selectors
- Page structure
- Recommended approach
```

### Example 4: Request New Website
```
/actionbook-scraper:request-website https://newsite.com/data

Action: Submits form to actionbook.app (this command DOES execute agent-browser)
```

## Best Practices

1. **Always analyze before generating** - Understand the page structure first
2. **Check list-sources** - Verify the site is indexed before attempting
3. **Review generated code** - Verify selectors match expected elements
4. **Add appropriate delays** - Be respectful to target servers
5. **Handle edge cases** - Empty states, loading states, errors
6. **Test incrementally** - Run on small subset before full scrape

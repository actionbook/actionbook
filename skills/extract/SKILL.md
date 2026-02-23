---
name: extract
description: Extract structured data from websites and produce an executable Playwright script plus extracted data. Use when the user wants to scrape, extract, pull, collect, or harvest data from any website — product listings, tables, search results, feeds, profiles, or any repeating content.
---

## When to Use This Skill

Activate when the user wants to **obtain data** from a website:

- "Extract all product prices from this page"
- "Scrape the table of results from ..."
- "Pull the list of authors and titles from arXiv search results"
- "Collect all job listings from this page"
- "Get the data from this dashboard table"
- "Harvest review scores from ..."
- "Download all the links/images/cards from ..."

The deliverable is always **two artifacts**:

1. **Executable Playwright script** — a standalone `.cjs` file that reproduces the extraction without Actionbook at runtime.
2. **Extracted data** — JSON (default), CSV, or user-specified format written to disk.

## Decision Strategy

Use Actionbook as a **conditional accelerator**, not a mandatory step. The goal is reliable selectors in the shortest path.

```
User request
  │
  ├─► actionbook search "<site> <intent>"
  │     ├─ Results with Health Score ≥ 70%  ──► actionbook get "<ID>" ──► use selectors
  │     └─ No results / low score  ──► Fallback
  │
  └─► Fallback: actionbook browser open <url>
        ├─ actionbook browser snapshot   (accessibility tree → find selectors)
        ├─ actionbook browser screenshot (visual confirmation)
        └─ manual selector discovery via DOM inspection
```

**Priority order for selector sources:**

| Priority | Source | When |
|----------|--------|------|
| 1 | `actionbook get` | Site is indexed, health score ≥ 70% |
| 2 | `actionbook browser snapshot` | Not indexed or selectors outdated |
| 3 | DOM inspection via screenshot + snapshot | Complex SPA / dynamic content |

**Non-negotiable rule:** if `search + get` already provides usable selectors for required fields, start from `get` selectors and do not jump to full fallback (`snapshot`/`screenshot`) by default. Exception: lightweight mechanism probes (for hydration/virtualization/pagination) are allowed when runtime behavior may affect script correctness. Escalate to `snapshot`/`screenshot` only when probes/sample validation indicate selector gaps or instability.

## Mechanism-Aware Script Strategy

Websites use patterns that break naive scraping. The generated Playwright script **must** account for these:

### Streaming / SSR / RSC hydration

Pages may render a shell first, then stream or hydrate content.

```javascript
// Wait for hydration to complete — not just DOMContentLoaded
await page.waitForSelector('[data-item]', { state: 'attached' });
await page.waitForFunction(() => {
  const items = document.querySelectorAll('[data-item]');
  return items.length > 0 && !document.querySelector('[data-pending]');
});
```

**Detection cues:** React root with `data-reactroot`, Next.js `__NEXT_DATA__`, empty containers that fill after JS runs. If `actionbook browser text "<selector>"` returns empty but the screenshot shows content, hydration hasn't completed.

### Virtualized lists / virtual DOM

Only visible rows exist in the DOM. Scrolling renders new rows and destroys old ones.

```javascript
// Scroll-and-collect loop for virtualized lists (scroll container aware)
const allItems = [];
const maxScrolls = 50;
let scrolls = 0;

const container = await page.$('<scroll-container-selector>');
if (!container) throw new Error('Scroll container not found');

let previousTop = await container.evaluate(el => el.scrollTop);
while (scrolls < maxScrolls) {
  const items = await page.$$eval('[data-row]', rows =>
    rows.map(r => ({ text: r.textContent.trim() }))
  );
  for (const item of items) {
    if (!allItems.find(i => i.text === item.text)) allItems.push(item);
  }

  await container.evaluate(el => el.scrollBy(0, 600));
  await page.waitForTimeout(300);

  const currentTop = await container.evaluate(el => el.scrollTop);
  if (currentTop === previousTop) break;

  previousTop = currentTop;
  scrolls += 1;
}
```

**Detection cues:** Container has fixed height with `overflow: auto/scroll`, row count in DOM is much smaller than stated total, rows have `transform: translateY(...)` or `position: absolute; top: ...px`.

### Infinite scroll / lazy loading

New content appends when the user scrolls near the bottom.

```javascript
// Scroll to bottom until no new content loads (with no-growth tolerance)
let itemCount = 0;
let noGrowthStreak = 0;
const maxScrolls = 80;
let scrolls = 0;

while (scrolls < maxScrolls && noGrowthStreak < 3) {
  await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
  await page.waitForTimeout(1200);

  const newCount = await page.$$eval('.item', els => els.length);
  if (newCount > itemCount) {
    itemCount = newCount;
    noGrowthStreak = 0;
  } else {
    noGrowthStreak += 1;
  }

  scrolls += 1;
}
```

**Detection cues:** Intersection Observer in page JS, "Load more" button, sentinel element at bottom, network requests firing on scroll.

### Pagination

Multi-page results behind "Next" buttons or numbered pages.

```javascript
// Click-through pagination (navigation-aware, SPA-safe)
const allData = [];
const maxPages = 50;
let pageIndex = 0;
while (pageIndex < maxPages) {
  const pageData = await page.$$eval('.result-item', items =>
    items.map(el => ({ title: el.querySelector('h3')?.textContent?.trim() }))
  );
  allData.push(...pageData);

  const nextBtn = await page.$('a.next-page:not([disabled])');
  if (!nextBtn) break;

  const previousUrl = page.url();
  const previousFirstItem = await page
    .$eval('.result-item', el => el.textContent?.trim() || '')
    .catch(() => '');

  await nextBtn.click();

  // Post-click detection only: advance must be caused by this click
  const advanced = await Promise.any([
    page
      .waitForURL(url => url.toString() !== previousUrl, { timeout: 5000 })
      .then(() => true),
    page
      .waitForFunction(
        prev => {
          const first = document.querySelector('.result-item');
          return !!first && (first.textContent || '').trim() !== prev;
        },
        previousFirstItem,
        { timeout: 5000 }
      )
      .then(() => true),
  ]).catch(() => false);

  if (!advanced) break;

  await page.waitForLoadState('networkidle').catch(() => {});
  pageIndex += 1;
}
```

## Execution Chain

### Step 1: Understand the target

Identify from the user request:
- **URL** — the page to extract from
- **Data shape** — what fields / columns are needed
- **Scope** — single page, paginated, infinite scroll, or multi-page crawl
- **Output format** — JSON (default), CSV, or other

### Step 2: Obtain selectors and choose execution path

```bash
# Try Actionbook index first
actionbook search "<site> <data-description>" --domain <domain>

# If good results (health ≥ 70%), get full selectors
actionbook get "<ID>"
```

Use this routing strictly:

- **Path A (default when `get` is good):** requested fields are covered by `get` selectors and quality is acceptable.
  - Start from `get` selectors and move to script draft quickly.
  - You may run lightweight mechanism probes (`browser text`, quick scroll checks) before finalizing script strategy.
  - **Do not run full fallback (`snapshot` / `screenshot`) before first draft unless probe/sample validation shows mismatch.**
  - Field mapping must default to `get` selectors and mark source as `actionbook_get`.

- **Path B (partial / unstable):** `get` exists but required fields are missing, selector resolves 0 elements, or validation fails.
  - Run targeted fallback only for failed fields/steps.

- **Path C (no usable coverage):** search/get has no usable result.
  - Run full fallback discovery.

### Step 3: Probe page mechanisms and fallback only when needed

Path A mechanism detection timing:
- Run minimal probes either **before final script draft** or during **sample validation**.
- Before any probe command, ensure the correct page context is open:
  - `actionbook browser open "<url>"` (if current tab context is unknown/stale)
- If probes/sample run indicate mismatch (missing rows, unstable selectors, wrong pagination behavior), escalate to Path B targeted fallback.

Fallback discovery by path:

**Path B targeted fallback (only failed fields/steps):**

```bash
actionbook browser open "<url>"     # if not already open
actionbook browser snapshot          # focus on failed field/container mapping
# actionbook browser screenshot      # optional visual confirmation for failed area
```

**Path C full fallback (no usable coverage):**

```bash
actionbook browser open "<url>"
actionbook browser snapshot
actionbook browser screenshot
```

Mechanism probes (run when script strategy needs confirmation):

```bash
# Hydration / streaming check
actionbook browser text "<container-selector>"

# Infinite scroll quick signal (explicit before/after decision)
actionbook browser eval "document.querySelectorAll('<item-selector>').length"   # before
actionbook browser click "<scroll-container-selector-or-body>"                    # focus scroll context
actionbook browser eval "const c=document.querySelector('<scroll-container-selector>') || document.scrollingElement; c.scrollBy(0, c.clientHeight || window.innerHeight);"
actionbook browser eval "document.querySelectorAll('<item-selector>').length"   # after
# If count increases, treat page as lazy-load/infinite-scroll.
```

Fallback trigger conditions:
- `actionbook get` cannot map all required fields.
- `actionbook get` selectors return empty/unstable values in sample run.
- Runtime behavior conflicts with expected mechanism (e.g., virtualized container, delayed hydration).


### Step 4: Generate Playwright script

Write a standalone Playwright script (`extract_<domain>_<slug>.cjs`) that:

1. Navigates to the target URL.
2. Waits for the correct readiness signal (not just `load` — see mechanisms above).
3. Handles the detected mechanism (virtual scroll, pagination, etc.).
4. Extracts data into structured objects.
5. Writes output to disk (`JSON.stringify` / CSV).
6. Closes the browser.
7. Enforces guardrails (`maxPages`, `maxScrolls`, timeout budget) to avoid infinite loops.

**Script template:**

```javascript
// extract_<domain>_<slug>.cjs
// Generated by Actionbook extract skill
// Usage: node extract_<domain>_<slug>.cjs

const { chromium } = require('playwright');

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage();

  await page.goto('<URL>', { waitUntil: 'domcontentloaded' });

  // -- wait for readiness --
  await page.waitForSelector('<container>', { state: 'visible' });

  // -- extract --
  const data = await page.$$eval('<item-selector>', items =>
    items.map(el => ({
      // fields mapped from user request
    }))
  );

  // -- output --
  const fs = require('fs');
  fs.writeFileSync('output.json', JSON.stringify(data, null, 2));
  console.log(`Extracted ${data.length} items → output.json`);

  await browser.close();
})();
```

### Step 5: Execute and validate

Run the script to confirm it works:

```bash
node extract_<domain>_<slug>.cjs
```

**Validation rules:**

| Check | Pass condition |
|-------|---------------|
| Script exits 0 | No runtime errors |
| Output file exists | Non-empty file written |
| Record count > 0 | At least one item extracted |
| No null/empty fields | Every declared field has a value in ≥ 90% of records |
| Data matches page | Spot-check first and last record against `actionbook browser text` |

If validation fails, inspect the output, adjust selectors or wait strategy, and re-run.

### Step 6: Deliver

Present to the user:
1. **Script path** — the `.cjs` file they can re-run anytime.
2. **Data path** — the output JSON/CSV file.
3. **Record count** — how many items were extracted.
4. **Notes** — any mechanism-specific caveats (e.g., "this site uses infinite scroll; the script scrolls up to 50 pages by default").

## Output Contract

Every `extract` invocation produces:

| Artifact | Path | Format |
|----------|------|--------|
| Playwright script | `./extract_<domain>_<slug>.cjs` | Standalone Node.js script using `playwright` |
| Extracted data | `./output.json` (default) or user-specified path | JSON array of objects (default), CSV, or user-specified |

The script must be **re-runnable** — a user should be able to execute it later without Actionbook installed, as long as Node.js + Playwright are available in the runtime environment.

## Selector Priority

When multiple selector types are available from `actionbook get`:

| Priority | Type | Reason |
|----------|------|--------|
| 1 | `data-testid` | Stable, test-oriented, rarely changes |
| 2 | `aria-label` | Accessibility-driven, semantically meaningful |
| 3 | CSS selector | Structural, may break on redesign |
| 4 | XPath | Last resort, most brittle |

## Error Handling

| Error | Action |
|-------|--------|
| `actionbook search` returns no results | Fall back to `snapshot` + `screenshot` |
| Selector returns 0 elements | Re-snapshot, compare with screenshot, update selector |
| Script times out | Add longer `waitForTimeout`, check for anti-bot measures |
| Partial data (some fields empty) | Check if content is lazy-loaded; add scroll/wait |
| Anti-bot / CAPTCHA | Inform user; suggest running with `headless: false` or using their own browser session via `actionbook setup` extension mode |

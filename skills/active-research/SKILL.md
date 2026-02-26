---
name: active-research
description: Deep research and analysis tool. Generates comprehensive HTML reports on any topic, domain, paper, or technology. Enhanced with advanced browser automation — SPA handling, network idle wait, batch operations, stealth browsing, and intelligent page analysis. Use when user asks to research, analyze, investigate, deep-dive, or generate a report on any subject.
exclude_tools: group:web
triggers: research, deep-dive, investigate, analyze topic, deep research, generate report
force_tool_turns: 10
---

# Active Research

Analyze any topic, domain, or paper and generate a beautiful HTML report using **Actionbook Browser** — featuring SPA-aware navigation, network idle detection, batch operations, and intelligent page analysis.

## Enhanced Browser Capabilities

| Capability | Description |
|------------|-------------|
| Page load wait | `wait-idle` — monitors fetch/XHR until network settles |
| SPA content | `wait-fn` — wait for JS conditions before extracting |
| Page understanding | `snapshot --filter interactive --max-tokens N` — focused, budget-friendly |
| Popups blocking | `--auto-dismiss-dialogs` — auto-handle alert/confirm/prompt |
| Load speed | `--block-images` — skip images for faster text extraction |
| Page stability | `--no-animations` — freeze CSS transitions |
| Error detection | `console --level error` — check for page issues |
| Multi-step forms | `batch` — execute multiple actions in one call |
| Element debugging | `info <selector>` — inspect visibility, position, properties |
| Change tracking | `snapshot --diff` — only see what changed |
| Anti-detection | `--stealth` + `fingerprint rotate` for protected sites |
| Auth management | `storage set` — inject JWT/tokens for gated content |
| One-shot fetch | `browser fetch <url>` — navigate+wait+extract+close in one command |
| Static page speed | `--lite` — HTTP-first, browser fallback only if needed |
| Anti-scrape URLs | `--rewrite-urls` — x.com→xcancel.com, reddit→old.reddit |
| Wait tuning | `--wait-hint` — domain-aware wait (fast/normal/slow/heavy) |
| Log correlation | `--session-tag` — tag all operations for debugging |

## Usage

```
/active-research <topic>
/active-research <topic> --output ./reports/my-report.json
```

Or simply tell Claude: "Research XXX and generate a report"

### Parameters

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `topic` | Yes | - | The subject to research (any text) |
| `--output` | No | `./output/<topic-slug>.json` | Output path for JSON report |

### Topic Detection

| Pattern | Type | Strategy |
|---------|------|----------|
| `arxiv:XXXX.XXXXX` | Paper | **arXiv Advanced Search** + ar5iv deep read |
| `doi:10.XXX/...` | Paper | Resolve DOI, then **arXiv Advanced Search** for related work |
| Academic keywords (paper, research, model, algorithm) | Academic topic | **arXiv Advanced Search** + Google for non-academic sources |
| URL | Specific page | Fetch and analyze the page |
| General text | Topic research | Google search + arXiv Advanced Search if relevant |

## Architecture

```
┌──────────┐     ┌──────────────┐     ┌──────────────┐     ┌──────────┐
│  Claude   │────▶│  Actionbook  │────▶│  Web Pages   │────▶│ Extract  │
│  Code     │     │   Browser    │     │  (multiple)  │     │ Content  │
└──────────┘     └──────────────┘     └──────────────┘     └─────┬────┘
      │           │ wait-idle    │     │ SPA / dynamic │           │
      │           │ batch ops    │     │ protected     │           │
      │           │ --stealth    │     │ mobile-only   │           │
      │           │ snapshot     │     └───────────────┘           │
      │           └──────────────┘                                 │
      │          ┌──────────────┐     ┌──────────────┐            │
      ├─────────▶│  Actionbook  │     │ arXiv Adv.   │            │
      │          │  search/get  │────▶│ Search Form  │───────────▶│
      │          │  (selectors) │     │ (40+ fields) │            │
      │          └──────────────┘     └──────────────┘            │
      │                                                            │
┌──────────┐     ┌──────────────┐     ┌──────────────┐            │
│  Open in │◀────│   json-ui    │◀────│  Write JSON  │◀───────────┘
│  Browser │     │   render     │     │  Report      │  Synthesize
└──────────┘     └──────────────┘     └──────────────┘
```

## MUST USE Actionbook CLI

**Always use `actionbook browser` commands for web browsing. NEVER use any other method to access the web:**

- **NEVER** use `curl`, `wget`, `httpie`, or any HTTP CLI tool via bash
- **NEVER** use `python -c "import requests"` or any scripting-language HTTP library via bash
- **NEVER** use WebFetch or WebSearch tools
- **ONLY** use `actionbook browser` and `actionbook search`/`actionbook get` commands

If you need web content, the PREFERRED path is: `actionbook browser fetch <url> --format text --json` (one-shot).
For interactive multi-step workflows, use: `actionbook browser open <url>` → `actionbook browser wait-idle` → `actionbook browser text`.

## Browser Flags — Research Defaults

**CRITICAL: Always use these flags when opening the browser for research.**

```bash
# PREFERRED: One-shot fetch (I1) — handles open+wait+extract+close automatically
actionbook --block-images --rewrite-urls browser fetch "<url>" --format text --json

# For interactive multi-step workflows, use explicit open:
actionbook --block-images --auto-dismiss-dialogs --no-animations --rewrite-urls browser open "<url>"
```

| Flag | Why |
|------|-----|
| `--block-images` | Skip image downloads — 2-5x faster page load for text extraction |
| `--auto-dismiss-dialogs` | Prevent alert/confirm/prompt from blocking automation |
| `--no-animations` | Freeze CSS animations — stable snapshots, no timing issues |
| `--rewrite-urls` | Rewrite x.com→xcancel.com, reddit→old.reddit to avoid anti-bot blocking |
| `--wait-hint <hint>` | Domain-aware wait: `instant`, `fast`, `normal`, `slow`, `heavy`, or ms |
| `--session-tag <tag>` | Tag all operations for log correlation and debugging |
| `--lite` (fetch only) | Try HTTP first, skip browser for static pages (Wikipedia, docs, blogs) |

For sites with anti-bot protection, add `--stealth`:
```bash
actionbook --block-images --auto-dismiss-dialogs --no-animations --stealth --rewrite-urls browser open "<url>"
```

## Navigation Pattern — ALWAYS Follow

**Option A: One-shot fetch (PREFERRED for read-only page extraction):**

```bash
# Single command: navigate → wait (domain-aware) → extract → close
actionbook --block-images --rewrite-urls browser fetch "<url>" --format text --json
# For static pages (Wikipedia, docs, blogs), add --lite to skip browser entirely:
actionbook --rewrite-urls browser fetch "<url>" --format text --lite --json
# For accessibility tree:
actionbook --block-images --rewrite-urls browser fetch "<url>" --format snapshot --max-tokens 2000 --json
```

**Option B: Interactive multi-step pattern (for forms, clicks, multi-page flows):**

```bash
# Step 1: Navigate
actionbook browser open "<url>"          # or: goto, click a link

# Step 2: Wait for load (MANDATORY in v2)
actionbook browser wait-idle             # Wait for fetch/XHR to settle

# Step 3: Extract content
actionbook browser text [selector]       # Extract text
# OR
actionbook browser snapshot --filter interactive --max-tokens 500  # Understand page structure
```

**Why `wait-idle` is critical:**
- SPAs (React, Vue, Next.js) load content via fetch/XHR after initial HTML
- Without waiting, `text` returns empty or incomplete content
- `wait-idle` monitors all pending network requests, waits until quiet for 500ms

**For pages that load content dynamically after network settles:**
```bash
actionbook browser wait-idle
actionbook browser wait-fn "document.querySelector('.results')"    # Wait for specific element
actionbook browser text ".results"
```

## Complete Workflow

> **REMINDER:** Every web access in this workflow MUST use `actionbook browser` commands.
> Using `curl`, `wget`, `python requests`, or any other HTTP tool is **strictly forbidden**.
> The bash tool should ONLY be used for `actionbook` CLI commands and local file operations (json-ui render, `open`).

### Step 1: Plan Search Strategy

Based on the topic, generate 5-8 search queries from different angles:
- Core definition / overview
- Latest developments / news
- Technical details / implementation
- Comparisons / alternatives
- Expert opinions / analysis
- Use cases / applications

**Search order — ALWAYS query Actionbook API first, then search:**

| Step | Action | Why |
|------|--------|-----|
| **Step 2 (FIRST)** | **Query Actionbook API** | Get verified selectors for arXiv, ar5iv, and other known sites BEFORE browsing. |
| **Step 3 (SECOND)** | **arXiv Advanced Search** | Use Actionbook selectors for multi-field, filtered academic search. |
| **Step 4 (THIRD)** | Google / Bing search | Supplement with blogs, news, code, discussions, non-academic sources. |

### Step 2: Query Actionbook API for Selectors (ALWAYS DO THIS FIRST)

**BEFORE browsing any URL, query Actionbook's indexed selectors.**

```bash
# Search for indexed actions by domain
actionbook search "<keywords>" -d "<domain>"

# Get detailed selectors for a specific page
actionbook get "<domain>:/<path>:<area>"
```

**Pre-indexed sites useful for research:**

| Site | area_id | Key Selectors |
|------|---------|---------------|
| arXiv Advanced Search | `arxiv.org:/search/advanced:default` | **40+ selectors**: field select, term input, category checkboxes, date range filters |
| ar5iv paper | `ar5iv.labs.arxiv.org:/html/{paper_id}:default` | `h1.ltx_title_document`, `div.ltx_authors`, `div.ltx_abstract`, `section.ltx_section` |
| Google Scholar | `scholar.google.com:/:default` | `#gs_hdr_tsi` (search), `#gs_hdr_tsb` (submit) |
| arXiv homepage | `arxiv.org:/:default` | Global search across 2.4M+ articles |

**For any URL you plan to visit**, run `actionbook search "<keywords>" -d "<domain>"` to check if it's indexed.

### Step 3: arXiv Search (URL-First, Form as Backup)

**LESSON LEARNED: arXiv form submission via browser automation is unreliable. Use URL-based search as the PRIMARY method.**

**Option A: URL-based search (PRIMARY — most reliable):**

```bash
# Simple keyword search
actionbook --block-images --auto-dismiss-dialogs --no-animations browser open "https://arxiv.org/search/?query=large+language+model+agent&searchtype=all"
actionbook browser wait-idle
actionbook browser text "#main-container"

# Advanced URL search with filters
# searchtype: all, title, author, abstract
# start: result offset (0, 50, 100, ...)
actionbook browser open "https://arxiv.org/search/?query=Rust+machine+learning&searchtype=all&start=0"
actionbook browser wait-idle
actionbook browser text "#main-container"
```

**Search strategy: Start broad, then narrow:**
1. First search: broad terms (e.g., `"Rust" "machine learning"`) — aim for 50+ results
2. If too few results (< 10): broaden further, remove date/category filters
3. If too many results (> 200): add more specific terms, use `searchtype=title`
4. Try 2-3 different query angles (e.g., framework names, use cases, benchmarks)

**Option B: Form interaction via batch (BACKUP — use if URL search is insufficient):**

```bash
# Open arXiv with research flags
actionbook --block-images --auto-dismiss-dialogs --no-animations browser open "https://arxiv.org/search/advanced"
actionbook browser wait-idle

# Use batch for form — fewer round-trips, more reliable
cat <<'EOF' | actionbook browser batch --delay 150
{
  "actions": [
    {"kind": "click", "selector": "#terms-0-field"},
    {"kind": "click", "selector": "option[value='title']"},
    {"kind": "type", "selector": "#terms-0-term", "text": "large language model agent"},
    {"kind": "click", "selector": "#classification-computer_science"},
    {"kind": "click", "selector": "#date-filter_by-3"},
    {"kind": "type", "selector": "#date-from_date", "text": "2025-01-01"},
    {"kind": "type", "selector": "#date-to_date", "text": "2026-02-23"},
    {"kind": "click", "selector": "button:has-text('Search'):nth(2)"}
  ],
  "stopOnError": true
}
EOF
actionbook browser wait-idle
actionbook browser text "#main-container"

# If batch form submission fails (page shows form again instead of results):
# → Fall back to Option A URL-based search immediately
# → Do NOT retry the form — it wastes time
```

**arXiv search capabilities (from indexed selectors — for Option B):**

| Capability | Selector |
|------------|----------|
| Search field (Title/Author/Abstract) | `#terms-0-field` select |
| Search term | `#terms-0-term` input |
| Add boolean terms | `button "Add another term +"` |
| Filter: Computer Science | `#classification-computer_science` |
| Filter: Physics, Math, etc. | `#classification-physics`, `#classification-mathematics` |
| Date: past 12 months | `#date-filter_by-1` radio |
| Date: specific year | `#date-filter_by-2` radio + `#date-year` |
| Date: custom range | `#date-filter_by-3` radio + `#date-from_date` / `#date-to_date` |
| Show abstracts | `#abstracts-0` radio |

### Step 4: Supplement with Google / Bing Search

```bash
# Search via Google (with wait-idle for SPA results)
actionbook browser open "https://www.google.com/search?q=<encoded_query>"
actionbook browser wait-idle
actionbook browser text "#search"

# Or search via Bing
actionbook browser open "https://www.bing.com/search?q=<encoded_query>"
actionbook browser wait-idle
actionbook browser text "#b_results"
```

Parse search results to extract URLs. For each discovered URL, query Actionbook API to check if indexed.

**CRITICAL: URL Handling Rules (Learned from Production Use)**

1. **NEVER manually construct URLs from search snippets.** Many Google snippet URLs are truncated or reformatted. Instead:
   - Use `actionbook browser snapshot --filter interactive` to find actual link elements
   - Click the link directly: `actionbook browser click "a[href*='domain.com']"`
   - Or extract href from snapshot refs

2. **Expect 20-30% of URLs to be dead.** In practice, ~5 out of 20 URLs return 404. Handle this:
   ```bash
   actionbook browser open "<url>"
   actionbook browser wait-idle
   # Check if the page is a 404 or error page
   actionbook browser wait-fn "!document.title.includes('404') && !document.title.includes('Not Found')" --timeout 3000
   # If timeout → page is dead, skip immediately. Do NOT retry.
   ```

3. **Salvage info from Google snippets.** If a URL is dead but the Google snippet had useful info:
   - The snippet text you already extracted IS valid data
   - Use it in the report with a note that the source is no longer available
   - Search for the same content on alternative sites (archive.org, cached versions)

4. **Use 4+ diverse search queries.** Don't rely on one search angle:
   - Query 1: Core topic overview (e.g., "Rust AI ecosystem 2026")
   - Query 2: Specific frameworks/tools (e.g., "Candle vs Burn Rust ML framework")
   - Query 3: Use cases/benchmarks (e.g., "Rust LLM inference performance benchmark")
   - Query 4: Recent news/developments (e.g., "Rust machine learning latest 2026")
   - Query 5: Community/ecosystem (e.g., "Rust AI agent framework comparison")

### Step 5: Deep Read Sources

**PREFERRED: Use `browser fetch` for one-shot page extraction (handles wait + extract + cleanup):**

```bash
# Quick text extraction (most common)
actionbook --block-images --rewrite-urls browser fetch "<url>" --format text --json

# Static pages (Wikipedia, docs, blogs) — skip browser entirely
actionbook --rewrite-urls browser fetch "<url>" --format text --lite --json

# Page structure analysis
actionbook --block-images --rewrite-urls browser fetch "<url>" --format snapshot --max-tokens 2000 --json

# With token budget for LLM context management
actionbook --block-images --rewrite-urls browser fetch "<url>" --format text --max-tokens 4000 --json
```

**For interactive workflows (forms, clicks), fall back to multi-step:**

```bash
actionbook browser open "<url>"
actionbook browser wait-idle                    # MANDATORY: wait for network
actionbook browser text                         # Full page text (fallback)
actionbook browser text "<selector>"            # Use Actionbook selector if indexed
```

**If page content seems incomplete, debug:**

```bash
# Check for JS errors that might block rendering
actionbook browser console --level error

# Check if a specific element exists
actionbook browser wait-fn "document.querySelector('.content')" --timeout 5000

# Inspect element properties
actionbook browser info ".content"
```

**For arXiv papers**, try sources in this order:

```bash
# 1. arXiv abstract (most reliable) — use fetch
actionbook --block-images browser fetch "https://arxiv.org/abs/<arxiv_id>" --format text --json

# 2. HuggingFace papers page
actionbook --block-images browser fetch "https://huggingface.co/papers/<arxiv_id>" --format text --json

# 3. ar5iv HTML (structured, but fails on new papers) — use --lite for static HTML
actionbook browser fetch "https://ar5iv.org/html/<arxiv_id>" --format text --lite --json
# NOTE: if content too short, ar5iv didn't render. Fall back.

# 4. GitHub repo (from search results) — use fetch
actionbook --block-images browser fetch "<github_repo_url>" --format text --json
```

**For protected sites (Cloudflare, bot detection) — use interactive mode with stealth:**

```bash
actionbook --stealth --block-images --auto-dismiss-dialogs --rewrite-urls browser open "<protected_url>"
actionbook browser wait-idle
actionbook browser text
```

**For mobile-only content:**

```bash
actionbook browser emulate iphone-14
actionbook browser open "<url>"
actionbook browser wait-idle
actionbook browser text
```

**For Google Scholar** (indexed by Actionbook):

```bash
actionbook browser open "https://scholar.google.com"
actionbook browser wait-idle
actionbook browser click "#gs_hdr_tsi"
actionbook browser type "#gs_hdr_tsi" "<query>"
actionbook browser click "#gs_hdr_tsb"
actionbook browser wait-idle
actionbook browser text "#gs_res"
```

**For unindexed sites**, use snapshot to discover structure:

```bash
actionbook --block-images browser fetch "<url>" --format snapshot --max-tokens 800 --json
```

### Step 6: Synthesize Findings

Organize collected information into a coherent report:
1. Overview / Executive Summary
2. Key Findings
3. Detailed Analysis
4. Supporting Data / Evidence
5. Implications / Significance
6. Sources

### Step 7: Generate json-ui JSON Report

Write a JSON file following the `@actionbookdev/json-ui` schema. Use the Write tool.

**Output path:** `./output/<topic-slug>.json` (or user-specified `--output` path)

### Step 8: Render HTML

**CRITICAL: You MUST try ALL fallback methods before giving up. Do NOT stop at the first failure.**

**IMPORTANT: Always use ABSOLUTE paths for JSON_FILE and HTML_FILE.**

Try each method one by one until one succeeds:

```bash
# Method 1: Monorepo absolute path (most reliable if inside actionbook project)
node "$(git rev-parse --show-toplevel)/packages/json-ui/dist/cli.js" render /absolute/path/to/report.json -o /absolute/path/to/report.html

# Method 2: Global install (if user ran: cd packages/json-ui && npm link)
json-ui render /absolute/path/to/report.json -o /absolute/path/to/report.html

# Method 3: npx (if published to npm)
npx @actionbookdev/json-ui render /absolute/path/to/report.json -o /absolute/path/to/report.html
```

**NEVER give up silently.** If all methods fail, tell the user:
1. The JSON report is saved at `<path>`
2. To enable HTML rendering, run: `cd <actionbook-repo>/packages/json-ui && npm link`

### Step 9: Open in Browser

```bash
# macOS
open <report.html>

# Linux
xdg-open <report.html>
```

### Step 10: Close Browser

**Always close the browser when done:**

```bash
actionbook browser close
```

## Error Recovery Patterns

**Intelligent error recovery using advanced browser capabilities:**

### Pattern: Page Load Failure

```bash
# 1. Open page
actionbook browser open "<url>"
actionbook browser wait-idle --timeout 15000

# 2. Check for JS errors
actionbook browser console --level error
# If errors found → page is broken, skip to next source

# 3. Check if content rendered
actionbook browser wait-fn "document.body.innerText.length > 100" --timeout 5000
# If timeout → content didn't render, try fallback
```

### Pattern: Selector Not Found

```bash
# 1. Use snapshot to discover actual page structure
actionbook browser snapshot --filter interactive --max-tokens 800

# 2. Or inspect a specific area
actionbook browser info "<parent_selector>"
# Returns: suggested selectors, visibility, tag info

# 3. Adjust selector and retry
```

### Pattern: Anti-Bot Detection

```bash
# 1. If initial load returns CAPTCHA or access denied:
actionbook browser close

# 2. Reopen with stealth
actionbook --stealth --no-animations --auto-dismiss-dialogs browser open "<url>"
actionbook browser wait-idle

# 3. If still blocked, rotate fingerprint
actionbook browser fingerprint rotate --os windows
actionbook browser open "<url>"
actionbook browser wait-idle
```

### Pattern: SPA Content Not Loading

```bash
# 1. Wait for network
actionbook browser wait-idle --idle-time 1000 --timeout 15000

# 2. Wait for specific element
actionbook browser wait-fn "document.querySelector('.results')" --timeout 10000

# 3. If still empty, check console
actionbook browser console --level error

# 4. Try clicking a loading trigger
actionbook browser snapshot --filter interactive --max-tokens 300
# Look for "Load More", "Show Results", etc.
```

## Full Error Handling Reference

| Error | Recovery Strategy |
|-------|-------------------|
| Browser fails to open | `actionbook browser status`, retry + check `console --level error` |
| Page load timeout | `wait-idle --timeout 15000`, then `console --level error` to diagnose |
| **URL returns 404** | `wait-fn "!document.title.includes('404')"` to detect fast. **Skip immediately, do NOT retry.** Use Google snippet text as backup data. |
| **arXiv form submission fails** | Fall back to URL-based search: `arxiv.org/search/?query=...&searchtype=all` |
| ar5iv content truncated | Fall back to arxiv abstract + `wait-fn "document.body.innerText.length > 5000"` to verify |
| Selector not found | `snapshot --filter interactive` to discover actual structure |
| Dynamic content missing | `wait-idle` + `wait-fn` for specific conditions |
| Alert popup blocking | `--auto-dismiss-dialogs` prevents this entirely |
| Anti-bot detection | `--stealth` + `fingerprint rotate` |
| Slow media-heavy page | `--block-images` or `--block-media` for 2-5x speedup |
| CSS animation interference | `--no-animations` freezes all transitions |
| json-ui render crash | Check MetricsGrid — `suffix`/`value` must be plain strings |
| `npx json-ui` 404 | Try all 3 methods (monorepo, global, npx) |
| No search results | Start broad (50+ results), then narrow. Use 4+ query angles. |

**IMPORTANT:** Always run `actionbook browser close` before finishing, even on errors.

## Feature Usage Checklist

**Before finalizing research, verify you used these capabilities:**

| Feature | When to Use | Check |
|---------|-------------|-------|
| `browser fetch` | Read-only page extraction (preferred over open+wait+text) | Use for most page reads |
| `--lite` | Static pages (Wikipedia, docs, blogs) — skip browser entirely | Add to `fetch` for static sites |
| `--rewrite-urls` | Always (avoids anti-bot on x.com, reddit) | Set in initial browser launch |
| `--wait-hint` | Domain-aware wait tuning (fast/slow/heavy) | Use with `fetch` or manual flow |
| `--session-tag` | Multi-step operations needing log correlation | Set for debugging sessions |
| `wait-idle` | After EVERY `open`/`goto`/`click` that triggers navigation | Must be used on every page |
| `--block-images` | Always (research doesn't need images) | Set in initial browser launch |
| `--auto-dismiss-dialogs` | Always (prevents blocking) | Set in initial browser launch |
| `--no-animations` | Always (stable snapshots) | Set in initial browser launch |
| `wait-fn` | When content loads asynchronously after network settles | Use on SPAs, dynamic pages |
| `console --level error` | When page content seems incomplete or broken | Use for debugging |
| `batch` | When filling multi-step forms (arXiv, Google Scholar) | Replaces 5+ sequential commands |
| `snapshot --filter interactive` | When discovering unknown page structure | Use on unindexed sites |
| `info <selector>` | When a click/type doesn't work as expected | Debug element visibility |
| `--stealth` | When site returns CAPTCHA or access denied | Add on retry |

**Common mistakes to avoid:**
- Using manual `open → wait-idle → text → close` when `browser fetch` does it in one command
- Not using `--lite` for static pages (Wikipedia, docs, blogs) — wastes 5-10s on browser startup
- Not using `--rewrite-urls` — x.com and reddit.com have aggressive anti-bot that blocks scraping
- Forgetting `wait-idle` after navigation in interactive mode (content appears empty)
- Not using `batch` for form interactions (slow, unreliable)
- Retrying a dead URL instead of skipping it
- Constructing URLs manually from search snippets instead of clicking links
- Using only one search query angle (always use 4+ diverse queries)
- Not checking for 404 pages before extracting content

## json-ui Report Template

**IMPORTANT: Always include BrandHeader and BrandFooter.**

```json
{
  "type": "Report",
  "props": { "theme": "auto" },
  "children": [
    {
      "type": "BrandHeader",
      "props": {
        "badge": "Deep Research Report",
        "poweredBy": "Actionbook"
      }
    },
    {
      "type": "Section",
      "props": { "title": "Overview", "icon": "paper" },
      "children": [
        {
          "type": "Prose",
          "props": {
            "content": "Overview of the topic..."
          }
        }
      ]
    },
    {
      "type": "Section",
      "props": { "title": "Key Findings", "icon": "star" },
      "children": [
        {
          "type": "ContributionList",
          "props": {
            "items": [
              {
                "badge": "Finding",
                "title": "...",
                "description": "..."
              }
            ]
          }
        }
      ]
    },
    {
      "type": "Section",
      "props": { "title": "Detailed Analysis", "icon": "bulb" },
      "children": [
        {
          "type": "Prose",
          "props": { "content": "..." }
        }
      ]
    },
    {
      "type": "Section",
      "props": { "title": "Key Metrics", "icon": "chart" },
      "children": [
        {
          "type": "MetricsGrid",
          "props": { "metrics": [], "cols": 3 }
        }
      ]
    },
    {
      "type": "Section",
      "props": { "title": "Sources", "icon": "link" },
      "children": [
        {
          "type": "LinkGroup",
          "props": { "links": [] }
        }
      ]
    },
    {
      "type": "BrandFooter",
      "props": {
        "timestamp": "YYYY-MM-DDTHH:MM:SSZ",
        "attribution": "Powered by Actionbook",
        "disclaimer": "This report was generated by AI using web sources. Verify critical information independently."
      }
    }
  ]
}
```

### Paper Report Template (for arXiv papers)

When analyzing academic papers, use a richer template with:
- `PaperHeader` (title, arxivId, date, categories)
- `AuthorList` (authors with affiliations)
- `Abstract` (with keyword highlights)
- `ContributionList` (key contributions)
- `MethodOverview` (step-by-step method)
- `ResultsTable` (experimental results)
- `Formula` (key equations, LaTeX)
- `Figure` (paper figures from ar5iv)

### Available json-ui Components

| Component | Use For | Key Props |
|-----------|---------|-----------|
| `BrandHeader` | Report header | `badge`, `poweredBy` |
| `PaperHeader` | Paper metadata | `title`, `arxivId`, `date`, `categories` |
| `AuthorList` | Authors | `authors: [{name, affiliation}]`, `maxVisible` |
| `Section` | Major section | `title`, `icon` (paper/star/bulb/chart/code/link/info/warning) |
| `Prose` | Rich text | `content` (supports **bold**, *italic*, `code`, lists) |
| `Abstract` | Abstract text | `text`, `highlights: ["keyword"]` |
| `ContributionList` | Numbered findings | `items: [{badge, title, description}]` |
| `MethodOverview` | Step-by-step | `steps: [{step, title, description}]` |
| `MetricsGrid` | Key stats | `metrics: [{label, value, trend, suffix}]`, `cols` |
| `ResultsTable` | Data table | `columns`, `rows`, `highlights: [{row, col}]` |
| `Table` | Generic table | `columns: [{key, label}]`, `rows`, `striped`, `compact` |
| `Callout` | Info/tip/warning | `type` (info/tip/warning/important/note), `title`, `content` |
| `Highlight` | Blockquote | `type` (quote/important/warning/code), `text`, `source` |
| `KeyPoint` | Key finding card | `icon`, `title`, `description`, `variant` |
| `CodeBlock` | Code snippet | `code`, `language`, `title`, `showLineNumbers` |
| `Formula` | LaTeX equation | `latex`, `block`, `label` |
| `Figure` | Image(s) | `images: [{src, alt, width}]`, `label`, `caption` |
| `Image` | Single image | `src`, `alt`, `caption`, `width` |
| `DefinitionList` | Term/definition | `items: [{term, definition}]` |
| `LinkGroup` | Source links | `links: [{href, label, icon}]` |
| `Grid` | Grid layout | `cols`, children |
| `Card` | Card container | `padding` (sm/md/lg), `shadow` |
| `TagList` | Tags | `tags: [{label, color, href}]` |
| `BrandFooter` | Footer | `timestamp`, `attribution`, `disclaimer` |

### json-ui Known Pitfalls

| Pitfall | Symptom | Fix |
|---------|---------|-----|
| `MetricsGrid.suffix` as object | `text.replace is not a function` | `suffix` must be a **plain string** |
| `MetricsGrid.value` as number | Render error | `value` must be a **string** (e.g., `"58.5"` not `58.5`) |
| Missing `BrandHeader`/`BrandFooter` | Report looks broken | Always include both |
| `Table` row values as object | `[object Object]` in cells | Row cell values must be **plain strings** |
| Very long Prose content | Truncated render | Split into multiple Prose blocks or use subsections |

### Text Fields

All text fields should use **plain English strings**.

```json
{ "title": "Key Findings" }
```

**Note:** `MetricsGrid` props `value` and `suffix`, and `Table` row cell values must always be plain strings.

## Academic Paper Support

### arXiv Papers

**ar5iv.org HTML** (preferred for reading, but often incomplete for papers < 3 months old):

| Element | Selector | Reliability | Fallback |
|---------|----------|-------------|----------|
| Title | `h1.ltx_title_document` | High | `div.ltx_abstract` |
| Authors | `div.ltx_authors` | High | — |
| Abstract | `div.ltx_abstract` | High | — |
| Full article | `article` | Medium | Use when section selectors fail |
| Sections | `section.ltx_section` | **Low on new papers** | `article` |
| Figures | `figure.ltx_figure` | Medium | — |
| Tables | `table.ltx_tabular` | Medium | — |

**Recommended approach:** Use `wait-idle` + `wait-fn` to verify ar5iv content loaded:

```bash
actionbook browser open "https://ar5iv.org/html/<arxiv_id>"
actionbook browser wait-idle --timeout 15000
actionbook browser wait-fn "document.body.innerText.length > 5000" --timeout 10000
# If wait-fn times out → content didn't render, fall back to other sources
```

### Recommended Source Priority

| Priority | Source | What you get | Reliability |
|----------|--------|-------------|-------------|
| 1 | `arxiv.org/abs/<id>` | Abstract, metadata, submission history | Very high |
| 2 | `huggingface.co/papers/<id>` | Abstract, community, related models | Very high |
| 3 | GitHub repo | README, code, model zoo | High |
| 4 | HuggingFace model card | Training recipe, benchmarks | High |
| 5 | `ar5iv.org/html/<id>` | Full paper HTML | Medium |
| 6 | Google Scholar / Semantic Scholar | Citations, related work | Medium |

### Other Academic Sources

- Google Scholar (`scholar.google.com`) — Actionbook indexed
- Semantic Scholar (`semanticscholar.org`)
- Papers With Code (`paperswithcode.com`)
- Conference proceedings sites

## Quality Guidelines

1. **Breadth**: Research from at least 3-5 diverse sources
2. **Depth**: Read full articles, not just snippets
3. **Accuracy**: Cross-reference facts across sources
4. **Structure**: Use appropriate json-ui components for each content type
5. **Attribution**: Always include source links in the report
6. **Freshness**: Prefer recent sources when relevance is equal

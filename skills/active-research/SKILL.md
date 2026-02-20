---
name: active-research
description: Deep research and analysis tool. Generates comprehensive HTML reports on any topic, domain, paper, or technology. Use when user asks to research, analyze, investigate, deep-dive, or generate a report on any subject. Supports academic papers (arXiv), technologies, trends, comparisons, and general topics.
---

# Active Research

Analyze any topic, domain, or paper and generate a beautiful HTML report using Actionbook browser automation and json-ui rendering.

## Usage

```
/active-research <topic>
/active-research <topic> --lang en
/active-research <topic> --lang zh
/active-research <topic> --lang both
/active-research <topic> --output ./reports/my-report.json
```

Or simply tell Claude: "Research XXX and generate a report"

### Parameters

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `topic` | Yes | - | The subject to research (any text) |
| `--lang` | No | `en` | Language mode: `en` (default), `zh`, or `both` |
| `--output` | No | `./output/<topic-slug>.json` | Output path for JSON report |

### Topic Detection

| Pattern | Type | Strategy |
|---------|------|----------|
| `arxiv:XXXX.XXXXX` | Paper | **arXiv Advanced Search** (Step 2b) + ar5iv deep read |
| `doi:10.XXX/...` | Paper | Resolve DOI, then **arXiv Advanced Search** for related work |
| Academic keywords (paper, research, model, algorithm) | Academic topic | **arXiv Advanced Search** (Step 2b) + Google for non-academic sources |
| URL | Specific page | Fetch and analyze the page |
| General text | Topic research | Google search + arXiv Advanced Search if relevant |

## Architecture

```
┌──────────┐     ┌──────────────┐     ┌──────────────┐     ┌──────────┐
│  Claude   │────▶│  Actionbook  │────▶│  Web Pages   │────▶│ Extract  │
│  Code     │     │  Browser CLI │     │  (multiple)  │     │ Content  │
└──────────┘     └──────────────┘     └──────────────┘     └─────┬────┘
      │                                                           │
      │          ┌──────────────┐     ┌──────────────┐           │
      ├─────────▶│  Actionbook  │     │ arXiv Adv.   │           │
      │          │  search/get  │────▶│ Search Form  │──────────▶│
      │          │  (selectors) │     │ (40+ fields) │           │
      │          └──────────────┘     └──────────────┘           │
      │                                                           │
      │    Actionbook indexes arXiv form selectors,               │
      │    enabling field-specific, filtered academic              │
      │    searches that WebFetch/WebSearch CANNOT do.             │
      │                                                           │
┌──────────┐     ┌──────────────┐     ┌──────────────┐           │
│  Open in │◀────│   json-ui    │◀────│  Write JSON  │◀──────────┘
│  Browser │     │   render     │     │  Report      │  Synthesize
└──────────┘     └──────────────┘     └──────────────┘
```

### Why Actionbook, Not WebFetch/WebSearch?

| Capability | Actionbook | WebFetch/WebSearch |
|------------|-----------|-------------------|
| Operate complex web forms (dropdowns, checkboxes, date pickers) | Yes — uses indexed selectors | No |
| arXiv: search by Author, Title, Abstract separately | Yes — `#terms-0-field` select | No — keyword only |
| arXiv: filter by subject (CS, Physics, Math, ...) | Yes — category checkboxes | No |
| arXiv: filter by date range or specific year | Yes — date inputs | No |
| Read pages with verified selectors (no guessing) | Yes — `actionbook get` | No — raw HTML parse |
| Interact with any indexed site's UI | Yes — click, type, select | No — read-only |

**This is the core value of Actionbook for research: it turns web forms into structured, programmable interfaces for AI agents.**

## MUST USE Actionbook CLI

**Always use `actionbook browser` commands for web browsing. Never use WebFetch or WebSearch.**

```bash
actionbook browser open <url>          # Navigate to page
actionbook browser snapshot            # Get accessibility tree
actionbook browser text [selector]     # Extract text content
actionbook browser screenshot [path]   # Capture visual
actionbook browser click <selector>    # Click element
actionbook browser close               # Close browser (ALWAYS do this at end)
```

## Complete Workflow

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
| **Step 2 (FIRST)** | **Query Actionbook API** | Get verified selectors for arXiv Advanced Search form, ar5iv papers, and any other known sites BEFORE browsing. This is the foundation for all subsequent steps. |
| **Step 3 (SECOND)** | **arXiv Advanced Search** | Use Actionbook selectors from Step 2 to perform multi-field, filtered academic search. Even non-academic topics often have relevant papers. |
| **Step 4 (THIRD)** | Google / Bing search | Supplement with blogs, news, code, discussions, non-academic sources. |

**IMPORTANT:** Always query Actionbook API first (Step 2) to get selectors, then use them in arXiv Advanced Search (Step 3). This is what makes Actionbook-powered research fundamentally different from WebFetch/WebSearch — the agent knows the exact selectors for every form field before it even opens the browser.

### Step 2: Query Actionbook API for Selectors (ALWAYS DO THIS FIRST)

**BEFORE browsing any URL, query Actionbook's indexed selectors.** This gives you verified CSS/XPath selectors instead of guessing.

```bash
# Search for indexed actions by domain
actionbook search "<keywords>" -d "<domain>"

# Get detailed selectors for a specific page
actionbook get "<domain>:/<path>:<area>"
```

**Pre-indexed sites useful for research:**

| Site | area_id | Key Selectors |
|------|---------|---------------|
| arXiv Advanced Search | `arxiv.org:/search/advanced:default` | **40+ selectors**: field select, term input, category checkboxes (CS/Physics/Math/...), date range filters, cross-list control — used in Step 3 |
| ar5iv paper | `ar5iv.labs.arxiv.org:/html/{paper_id}:default` | `h1.ltx_title_document` (title), `div.ltx_authors` (authors), `div.ltx_abstract` (abstract), `section.ltx_section` (sections) |
| Google Scholar | `scholar.google.com:/:default` | `#gs_hdr_tsi` (search input), `#gs_hdr_tsb` (search button) |
| arXiv homepage | `arxiv.org:/:default` | Global search across 2.4M+ articles |

**For any URL you plan to visit**, run `actionbook search "<keywords>" -d "<domain>"` to check if it's indexed. Use indexed selectors when available; fall back to `actionbook browser snapshot` for unindexed sites.

**Example: Get arXiv Advanced Search selectors before searching:**

```bash
# Query Actionbook for arXiv form selectors
actionbook get "arxiv.org:/search/advanced:default"
# Returns 40+ selectors: #terms-0-field, #terms-0-term, #classification-computer_science, etc.
```

### Step 3: arXiv Advanced Search (Using Actionbook Selectors)

> **Key differentiator:** WebFetch/WebSearch can only do simple keyword searches. Actionbook has indexed the **entire arXiv Advanced Search form** with 40+ verified selectors (queried in Step 2), enabling multi-field, multi-criteria academic searches — just like a human researcher would use the form.

Using the selectors obtained from Step 2, the Agent can:

| Capability | Actionbook Selector | WebFetch/WebSearch |
|------------|--------------------|--------------------|
| Search by specific field (Title, Author, Abstract) | `#terms-0-field` select → choose field | Not possible |
| Add multiple search terms with boolean logic | `button "Add another term +"` | Not possible |
| Filter by subject (CS, Physics, Math, etc.) | `#classification-computer_science` checkbox | Not possible |
| Filter by date range | `#date-filter_by-3` radio + `#date-from_date` / `#date-to_date` | Not possible |
| Filter by specific year | `#date-filter_by-2` radio + `#date-year` input | Not possible |
| Include/exclude cross-listed papers | `#classification-include_cross_list-0/1` radio | Not possible |
| Control results display | `#size` select, `#abstracts-0/1` radio | Not possible |

**Example: Search for recent CS papers by a specific author:**

```bash
# Open arXiv Advanced Search
actionbook browser open "https://arxiv.org/search/advanced"

# 1. Set search field to "Author" and type author name
actionbook browser click "#terms-0-field"
actionbook browser click "option[value='author']"
actionbook browser type "#terms-0-term" "Yann LeCun"

# 2. Filter to Computer Science only
actionbook browser click "#classification-computer_science"

# 3. Restrict to past 12 months
actionbook browser click "#date-filter_by-1"

# 4. Show abstracts in results
actionbook browser click "#abstracts-0"

# 5. Submit search
actionbook browser click "button:has-text('Search'):nth(2)"

# 6. Extract results
actionbook browser text "#main-container"
```

**Example: Search by title keywords in a date range:**

```bash
actionbook browser open "https://arxiv.org/search/advanced"

# Search in "Title" field
actionbook browser click "#terms-0-field"
actionbook browser click "option[value='title']"
actionbook browser type "#terms-0-term" "large language model agent"

# Date range: 2025-01 to 2026-02
actionbook browser click "#date-filter_by-3"
actionbook browser type "#date-from_date" "2025-01-01"
actionbook browser type "#date-to_date" "2026-02-09"

# Submit and extract
actionbook browser click "button:has-text('Search'):nth(2)"
actionbook browser text "#main-container"
```

### Step 4: Supplement with Google / Bing Search

After arXiv, use Google/Bing to find non-academic sources (blogs, news, docs, code, discussions):

```bash
# Search via Google
actionbook browser open "https://www.google.com/search?q=<encoded_query>"
actionbook browser text "#search"

# Or search via Bing
actionbook browser open "https://www.bing.com/search?q=<encoded_query>"
actionbook browser text "#b_results"
```

Parse the search results to extract URLs and snippets. Collect the top 5-10 most relevant URLs. For each discovered URL, query Actionbook API (Step 2 pattern) to check if the site is indexed before visiting.

### Step 5: Deep Read Sources

For each relevant URL, **first query Actionbook API** (same as Step 2) to check if the site is indexed, then use verified selectors:

```bash
actionbook browser open "<url>"
actionbook browser text                # Full page text (fallback)
actionbook browser text "<selector>"   # Use Actionbook selector if indexed
```

**For arXiv papers**, try sources in this order (newer papers often fail on ar5iv):

```bash
# 1. Try ar5iv first (best structured selectors from Actionbook)
actionbook browser open "https://ar5iv.org/html/<arxiv_id>"
actionbook browser text "h1.ltx_title_document"  # Title
actionbook browser text "div.ltx_authors"         # Authors
actionbook browser text "div.ltx_abstract"        # Abstract
# NOTE: section.ltx_section often fails on newer papers — use "article" as fallback

# 2. If ar5iv content is truncated (<5KB), fall back to arxiv abstract + other sources
actionbook browser open "https://arxiv.org/abs/<arxiv_id>"
actionbook browser text "main"

# 3. Supplement with HuggingFace model cards and GitHub READMEs for full details
actionbook browser open "https://huggingface.co/papers/<arxiv_id>"
actionbook browser text "main"
```

**Key lesson:** Don't rely solely on ar5iv. Always cross-reference 3-4 sources for completeness.

**For Google Scholar** (indexed by Actionbook):

```bash
actionbook browser open "https://scholar.google.com"
# Type into search: use selector #gs_hdr_tsi
actionbook browser click "#gs_hdr_tsi"
# ... type query, click #gs_hdr_tsb to search
```

**For unindexed sites**, use snapshot to discover page structure:

```bash
actionbook browser open "<url>"
actionbook browser snapshot            # Get accessibility tree to find selectors
actionbook browser text "<discovered_selector>"
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

**IMPORTANT: Always use ABSOLUTE paths for JSON_FILE and HTML_FILE.** Relative paths break when git rev-parse returns an absolute repo root.

Try each method one by one until one succeeds:

```bash
# Method 1: npx (recommended — works anywhere if npm is available)
npx @actionbookdev/json-ui render /absolute/path/to/report.json -o /absolute/path/to/report.html

# Method 2: Global install (if user ran: npm install -g @actionbookdev/json-ui)
json-ui render /absolute/path/to/report.json -o /absolute/path/to/report.html

# Method 3: Monorepo local path (fallback if inside actionbook project)
node "$(git rev-parse --show-toplevel)/packages/json-ui/dist/cli.js" render /absolute/path/to/report.json -o /absolute/path/to/report.html
```

**NEVER give up silently.** If all methods fail, tell the user:
1. The JSON report is saved at `<path>`
2. To install the renderer, run: `npm install -g @actionbookdev/json-ui`

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
            "content": "English overview..."
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

Optional: if a user explicitly asks for bilingual output, you can use i18n objects (see i18n section below).

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
| `MetricsGrid.suffix` as i18n object | `text.replace is not a function` | `suffix` must be a **plain string**, not `{ "en": ..., "zh": ... }` |
| `MetricsGrid.value` as number | Render error | `value` must be a **string** (e.g., `"58.5"` not `58.5`) |
| Missing `BrandHeader`/`BrandFooter` | Report looks broken | Always include both |
| `Table` row values as i18n object | `[object Object]` in cells | Row cell values must be **plain strings**. Column `label` supports i18n, but row data does not. Use `"Runtimes / Chinese label"` instead of `{ "en": "Runtimes", "zh": "Chinese label" }` |
| Very long Prose content | Truncated render | Split into multiple Prose blocks or use subsections |

### i18n Support

By default (`--lang en`), write plain strings:
```json
"English text"
```

If a user or agent explicitly chooses bilingual output (`--lang both`), use i18n objects:
```json
{ "en": "English text", "zh": "Chinese text" }
```

For `--lang zh`, use plain Chinese strings.

**Exceptions:**
- `MetricsGrid` props `value` and `suffix` must always be plain strings.
- `Table` row cell values must be plain strings (column `label` supports i18n, but row data does not). For bilingual rows, use a combined string like `"English / Chinese"`.

## Academic Paper Support

### arXiv Papers

**ar5iv.org HTML** (preferred for reading, but often incomplete for papers < 3 months old):

| Element | Selector (Actionbook-verified) | Reliability | Fallback |
|---------|-------------------------------|-------------|----------|
| Title | `h1.ltx_title_document` | High | `div.ltx_abstract` includes title context |
| Authors | `div.ltx_authors` | High | — |
| Abstract | `div.ltx_abstract` | High | — |
| Full article | `article` | Medium | Use when section selectors fail |
| Sections | `section.ltx_section` | **Low on new papers** | `article` for all content |
| Section title | `h2.ltx_title_section` | **Low on new papers** | Parse from `article` text |
| Figures | `figure.ltx_figure` | Medium | — |
| Tables | `table.ltx_tabular` | Medium | — |
| Bibliography | `.ltx_bibliography` | Medium | — |

**Note:** For papers submitted within the last ~3 months, ar5iv often renders incomplete content. Always check `actionbook browser text 2>&1 | wc -c` — if < 5KB, the page didn't fully render. Fall back to other sources.

**arXiv API** (for metadata via actionbook browser):
```
actionbook browser open "http://export.arxiv.org/api/query?id_list={arxiv_id}"
actionbook browser text
```

### Recommended Source Priority for Papers

Based on testing, use this priority order for maximum coverage:

| Priority | Source | What you get | Reliability |
|----------|--------|-------------|-------------|
| 1 | `arxiv.org/abs/<id>` | Abstract, metadata, submission history | Very high |
| 2 | `huggingface.co/papers/<id>` | Abstract, community comments, related models/datasets | Very high |
| 3 | GitHub repo (from search results) | README with method details, model zoo, code | High |
| 4 | HuggingFace model card | Training recipe, benchmark results, quick start | High |
| 5 | `ar5iv.org/html/<id>` | Full paper HTML with structured selectors | Medium (fails on new papers) |
| 6 | Google Scholar / Semantic Scholar | Citations, related work | Medium |

**Key insight:** Don't rely on a single source. The combination of arxiv abstract + HuggingFace + GitHub typically gives 90%+ of what you need, even when ar5iv fails.

### Other Academic Sources

Use `actionbook browser` to visit and extract content from:
- Google Scholar (`scholar.google.com`) — Actionbook indexed, use `#gs_hdr_tsi` for search
- Semantic Scholar (`semanticscholar.org`)
- Papers With Code (`paperswithcode.com`)
- Conference proceedings sites

## Error Handling

| Error | Action |
|-------|--------|
| Browser fails to open | Run `actionbook browser status`, retry |
| Page load timeout (30s) | Skip source, try next. Common on papers.cool, slow academic sites |
| ar5iv content truncated (<5KB) | Paper too new for ar5iv. Fall back to arxiv abstract + HuggingFace + GitHub |
| `section.ltx_section` not found | ar5iv rendering incomplete. Use `actionbook browser text "article"` or `"main"` instead |
| Actionbook selector not found | Use `actionbook browser snapshot` to discover actual page structure |
| `actionbook search` returns no results | Site not indexed. Use `actionbook browser snapshot` to find selectors manually |
| json-ui render crash (`text.replace`) | Check MetricsGrid `suffix`/`value` — must be plain strings, not i18n objects |
| `npx @actionbookdev/json-ui` fails | Run `npm install -g @actionbookdev/json-ui` and retry with `json-ui render`. If still fails, try monorepo local path |
| No search results | Broaden search terms, try different angles |
| Render failed | Save JSON, tell user path, and suggest: `npm install -g @actionbookdev/json-ui` |

**IMPORTANT:** Always run `actionbook browser close` before finishing, even on errors.

## Quality Guidelines

1. **Breadth**: Research from at least 3-5 diverse sources
2. **Depth**: Read full articles, not just snippets
3. **Accuracy**: Cross-reference facts across sources
4. **Structure**: Use appropriate json-ui components for each content type
5. **Attribution**: Always include source links in the report
6. **Freshness**: Prefer recent sources when relevance is equal

## Bilingual Content Notes (Optional)

When `--lang both` is used, the `zh` field should be written naturally (not word-by-word translation).

Practical rules:
1. Keep facts identical across languages (numbers, dates, source links).
2. Keep table text short and readable.
3. Preserve technical terms in English when there is no stable Chinese term.
4. Prioritize clarity over literal translation.

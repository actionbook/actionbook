---
name: deep-research
description: Deep research and analysis tool. Generates comprehensive HTML reports on any topic, domain, paper, or technology. Use when user asks to research, analyze, investigate, deep-dive, or generate a report on any subject. Supports academic papers (arXiv), technologies, trends, comparisons, and general topics.
---

# Deep Research

Analyze any topic, domain, or paper and generate a beautiful HTML report using Actionbook browser automation and json-ui rendering.

## Usage

```
/deep-research:analyze <topic>
/deep-research:analyze <topic> --lang zh
/deep-research:analyze <topic> --output ./reports/my-report.json
```

Or simply tell Claude: "帮我深度研究 XXX 并生成报告" / "Research XXX and generate a report"

### Parameters

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `topic` | Yes | - | The subject to research (any text) |
| `--lang` | No | `both` | Language: `en`, `zh`, or `both` (bilingual) |
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

**Decide your search method:**

| Topic type | Primary search | Secondary search |
|------------|---------------|-----------------|
| Academic paper / ML model / algorithm | **arXiv Advanced Search** (Step 2b) | Google for blogs, code, discussions |
| Specific author's recent work | **arXiv Advanced Search** by author field | Google Scholar |
| Papers in a subject + time range | **arXiv Advanced Search** with filters | — |
| Technology / product / general topic | Google Search (Step 2) | arXiv Advanced Search if research papers exist |
| News / trends / opinions | Google Search (Step 2) | — |

**IMPORTANT:** For any topic related to research, papers, models, or algorithms, you MUST use arXiv Advanced Search (Step 2b). This is Actionbook's key advantage — it can operate the arXiv search form with 40+ indexed selectors (field selection, category filters, date ranges) that no other tool can do.

### Step 2: Search the Web

**For general topics**, use Google/Bing:

```bash
# Search via Google
actionbook browser open "https://www.google.com/search?q=<encoded_query>"
actionbook browser text "#search"

# Or search via Bing
actionbook browser open "https://www.bing.com/search?q=<encoded_query>"
actionbook browser text "#b_results"
```

Parse the search results to extract URLs and snippets. Collect the top 5-10 most relevant URLs.

**For academic papers**, prefer arXiv Advanced Search (see Step 2b below).

### Step 2b: arXiv Advanced Search (Actionbook Advantage)

> **Key differentiator:** WebFetch/WebSearch can only do simple keyword searches. Actionbook has indexed the **entire arXiv Advanced Search form** with 40+ verified selectors, enabling multi-field, multi-criteria academic searches — just like a human researcher would use the form.

Actionbook knows every interactive element on the arXiv advanced search page (`arxiv.org:/search/advanced:default`). This lets the Agent:

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

**When to use arXiv Advanced Search vs. Google:**

| Scenario | Use |
|----------|-----|
| Know author name, need their recent papers | arXiv Advanced Search |
| Need papers in a specific subject + date range | arXiv Advanced Search |
| General topic exploration, multiple source types | Google Search |
| Looking for blog posts, news, non-academic sources | Google Search |
| Need both academic and non-academic sources | Google first, then arXiv Advanced Search |

### Step 3: Query Actionbook API for Known Site Selectors

**BEFORE browsing any URL, check if Actionbook has indexed the site's structure.** This gives you verified CSS/XPath selectors instead of guessing.

```bash
# Step 3a: Search for indexed actions by domain
actionbook search "<keywords>" -d "<domain>"

# Step 3b: Get detailed selectors for a specific page
actionbook get "<domain>:/<path>:<area>"
```

**Pre-indexed sites useful for research:**

| Site | area_id | Key Selectors |
|------|---------|---------------|
| ar5iv paper | `ar5iv.labs.arxiv.org:/html/{paper_id}:default` | `h1.ltx_title_document` (title), `div.ltx_authors` (authors), `div.ltx_abstract` (abstract), `section.ltx_section` (sections) |
| Google Scholar | `scholar.google.com:/:default` | `#gs_hdr_tsi` (search input), `#gs_hdr_tsb` (search button) |
| arXiv search | `arxiv.org:/search/advanced:default` | **40+ selectors**: field select, term input, category checkboxes (CS/Physics/Math/...), date range filters, cross-list control — see Step 2b |
| arXiv homepage | `arxiv.org:/:default` | Global search across 2.4M+ articles |

**For any other URL**, run `actionbook search "<keywords>" -d "<domain>"` to check if it's indexed. Use indexed selectors when available; fall back to `actionbook browser snapshot` for unindexed sites.

### Step 4: Deep Read Sources

For each relevant URL, use Actionbook-verified selectors when available:

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

### Step 5: Synthesize Findings

Organize collected information into a coherent report:
1. Overview / Executive Summary
2. Key Findings
3. Detailed Analysis
4. Supporting Data / Evidence
5. Implications / Significance
6. Sources

### Step 6: Generate json-ui JSON Report

Write a JSON file following the `@actionbookdev/json-ui` schema. Use the Write tool.

**Output path:** `./output/<topic-slug>.json` (or user-specified `--output` path)

### Step 7: Render HTML

Try these in order until one works:

```bash
# 1. Local monorepo (if running inside actionbook project)
node packages/json-ui/dist/cli.js render <report.json> -o <report.html>

# 2. npx (if published)
npx @actionbookdev/json-ui render <report.json> -o <report.html>

# 3. npx with @latest
npx @actionbookdev/json-ui@latest render <report.json> -o <report.html>
```

If all fail, save the JSON file and inform the user of its path.

### Step 8: Open in Browser

```bash
# macOS
open <report.html>

# Linux
xdg-open <report.html>
```

### Step 9: Close Browser

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
        "badge": { "en": "Deep Research Report", "zh": "深度研究报告" },
        "poweredBy": "Actionbook"
      }
    },
    {
      "type": "Section",
      "props": { "title": { "en": "Overview", "zh": "概述" }, "icon": "paper" },
      "children": [
        {
          "type": "Prose",
          "props": {
            "content": { "en": "English overview...", "zh": "中文概述..." }
          }
        }
      ]
    },
    {
      "type": "Section",
      "props": { "title": { "en": "Key Findings", "zh": "核心发现" }, "icon": "star" },
      "children": [
        {
          "type": "ContributionList",
          "props": {
            "items": [
              {
                "badge": { "en": "Finding", "zh": "发现" },
                "title": { "en": "...", "zh": "..." },
                "description": { "en": "...", "zh": "..." }
              }
            ]
          }
        }
      ]
    },
    {
      "type": "Section",
      "props": { "title": { "en": "Detailed Analysis", "zh": "详细分析" }, "icon": "bulb" },
      "children": [
        {
          "type": "Prose",
          "props": { "content": { "en": "...", "zh": "..." } }
        }
      ]
    },
    {
      "type": "Section",
      "props": { "title": { "en": "Key Metrics", "zh": "关键指标" }, "icon": "chart" },
      "children": [
        {
          "type": "MetricsGrid",
          "props": { "metrics": [], "cols": 3 }
        }
      ]
    },
    {
      "type": "Section",
      "props": { "title": { "en": "Sources", "zh": "信息来源" }, "icon": "link" },
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
        "disclaimer": {
          "en": "This report was generated by AI using web sources. Verify critical information independently.",
          "zh": "本报告由 AI 基于网络来源生成，请独立验证关键信息。"
        }
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
| `MetricsGrid.suffix` as i18n object | `text.replace is not a function` | `suffix` must be a **plain string**, not `{ "en": ..., "zh": ... }` |
| `MetricsGrid.value` as number | Render error | `value` must be a **string** (e.g., `"58.5"` not `58.5`) |
| Missing `BrandHeader`/`BrandFooter` | Report looks broken | Always include both |
| Very long Prose content | Truncated render | Split into multiple Prose blocks or use subsections |

### i18n Support

All text fields support bilingual output **unless noted above**:
```json
{ "en": "English text", "zh": "中文文本" }
```

For `--lang en`, use plain strings. For `--lang zh`, use plain Chinese strings. For `--lang both` (default), use i18n objects.

**Exception:** `MetricsGrid` props `value` and `suffix` must always be plain strings.

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
| `npx @actionbookdev/json-ui` 404 | Package not on npm. Use local: `node packages/json-ui/dist/cli.js render` |
| No search results | Broaden search terms, try different angles |
| Render failed | Save JSON and inform user of the path |

**IMPORTANT:** Always run `actionbook browser close` before finishing, even on errors.

## Quality Guidelines

1. **Breadth**: Research from at least 3-5 diverse sources
2. **Depth**: Read full articles, not just snippets
3. **Accuracy**: Cross-reference facts across sources
4. **Structure**: Use appropriate json-ui components for each content type
5. **Attribution**: Always include source links in the report
6. **Freshness**: Prefer recent sources when relevance is equal

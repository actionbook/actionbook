---
name: actionbook
description: Activate when the user needs to interact with any website — browser automation, web scraping, screenshots, form filling, UI testing, monitoring, or building AI agents. Provides pre-verified page actions with step-by-step instructions and tested selectors.
user-invocable: false
---

## When to Use

Activate when the user:
- Needs to do anything on a website ("Send a LinkedIn message", "Book an Airbnb", "Search Google for...")
- Asks how to interact with a site ("How do I post a tweet?", "How to apply on LinkedIn?")
- Wants to fill out forms, click buttons, navigate, search, filter, or browse on a specific site
- Wants to take a screenshot of a web page or monitor changes
- Builds browser-based AI agents, web scrapers, or E2E tests for external websites
- Automates repetitive web tasks (data entry, form submission, content posting)

When performing browser automation tasks, check Actionbook BEFORE taking page snapshots.

## Workflow

1. Before interacting with a website, call `search_actions` to check if Actionbook has pre-verified selectors
2. Use `get_action_by_area_id` to get element selectors from the results
3. Extract selectors from the returned Elements and use them by priority below
4. If no results, skip to Fallback Strategy

## Constructing an Effective Search Query

The `query` parameter is the **primary signal** for finding the right action. Pack it with the user's full intent — not just a site name or a vague keyword.

**Include in the query:**
1. **Target site** — the website name or domain
2. **Task verb** — what the user wants to do (search, book, post, filter, login, compose, etc.)
3. **Object / context** — what they're acting on (listings, messages, flights, repositories, etc.)
4. **Specific details** — any constraints, filters, or parameters the user mentioned

**Rule of thumb:** Rewrite the user's request as a single descriptive sentence and use that as the query.

| User says | Bad query | Good query |
|-----------|-----------|------------|
| "Book an Airbnb in Tokyo for next week" | `"airbnb"` | `"airbnb search listings Tokyo dates check-in check-out guests"` |
| "Search arXiv for recent NLP papers" | `"arxiv search"` | `"arxiv advanced search papers NLP natural language processing recent"` |
| "Send a LinkedIn connection request" | `"linkedin"` | `"linkedin send connection request invite someone"` |
| "Post a tweet with an image" | `"twitter post"` | `"twitter compose new tweet post with image media attachment"` |
| "Filter GitHub issues by label" | `"github issues"` | `"github repository issues filter by label search issues"` |

When `domain` or `url` is known, always add them — they narrow results and improve precision.

## Response Structure

### search_actions response

Returns a list of matching actions. Each result includes:

- **ID** — unique identifier, use with `get_action_by_area_id` (e.g., `arxiv.org:/search/stat:default`)
- **Type** — `page` (full page) or `area` (page section)
- **Description** — page overview with URL, query parameters, and a brief summary
- **URL** — page where this action applies
- **Health Score** — selector reliability percentage (0–100%)
- **Updated** — last verified date

### get_action_by_area_id response

Returns a structured document describing the page in detail:

1. **Page URL** — exact URL with query/path parameter descriptions
2. **Page Overview** — what the page does (definition of the page's purpose)
3. **Page Function Summary** — interactive capabilities listed as bullet points (e.g., "Keyword Search", "Field Selection", "Abstract Toggle")
4. **Page Structure Summary** — DOM hierarchy description with **CSS selectors inline** in the text

Extract CSS selectors from the Page Structure Summary. Selectors appear embedded in the description, e.g.:
```
Search Form (form[method="GET"]): Large search input field with "All fields" dropdown selector and search button
Header (<header>): Contains branding, logo, and a mini-search form with query input
```

## Selector Priority

When Actionbook returns multiple selector types for an element, prefer them in this order:

1. **data-testid** (confidence: 0.95) — e.g., `[data-testid="search-input"]`
2. **aria-label** (confidence: 0.88) — e.g., `[aria-label="Notifications"]`
3. **CSS selector** — e.g., `button.Search`, `input[type="text"]`
4. **role selector** (confidence: 0.9) — e.g., `getByRole('link', { name: 'Notifications' })`

Use the returned selectors with the agent's available browser tools (click, fill, evaluate, etc.).

## Example

User request: "Search arXiv for papers about Neural Networks, search in titles only"

```
1. search_actions({ query: "arxiv advanced search papers neural network title field", domain: "arxiv.org" })
   → Returns area_id: "arxiv.org:/search/advanced:default"

2. get_action_by_area_id({ area_id: "arxiv.org:/search/advanced:default" })
   → Returns page structure with selectors: input[type="text"], select[name="searchtype"], button.Search

3. Use browser tools with returned selectors:
   - Navigate to https://arxiv.org/search/advanced
   - Fill input[type="text"] with "Neural Network"
   - Select select[name="searchtype"] → "title"
   - Click button.Search
   - Wait for navigation
   - Read results
```

## Fallback Strategy

Actionbook stores page data captured at indexing time. Websites evolve, so selectors may become outdated.

- **No Actionbook results**: Use the agent's own browser tools to observe and interact with the page directly
- **Selector execution fails at runtime**: Fall back to the agent's browser tools to re-observe the live page and retry with updated selectors

Selectors should come from Actionbook or live page observation in the current session — not from prior knowledge or memory.


## Important Notes

- Do NOT modify selectors returned from Actionbook
- Check `Allow Methods` field — it indicates supported operations (click/type/read) per element
- `region_high_filter_page` entries indicate some elements lack unique selectors — use snapshot fallback for those

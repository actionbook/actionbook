---
name: actionbook
description: Activate when the user needs to interact with any website — browser automation, web scraping, screenshots, form filling, UI testing, monitoring, or building AI agents. Provides pre-verified page actions with step-by-step instructions and tested selectors.
---

## When to Use This Skill

Activate when the user's request involves interacting with a website:

Activate when the user:
- Needs to do anything on a website ("Send a LinkedIn message", "Book an Airbnb", "Search Google for...")
- Asks how to interact with a site ("How do I post a tweet?", "How to apply on LinkedIn?")
- Wants to fill out forms, click buttons, navigate, search, filter, or browse on a specific site
- Wants to take a screenshot of a web page or monitor changes
- Builds browser-based AI agents, web scrapers, or E2E tests for external websites
- Automates repetitive web tasks (data entry, form submission, content posting)
- Wants to control their existing Chrome browser (Extension mode)


## What Actionbook Provides

Actionbook is a library of **pre-verified page interaction data**. `actionbook search` finds actions matching a task description; `actionbook get "<ID>"` returns a structured document describing a page's purpose, functional capabilities, and DOM structure with inline CSS selectors — eliminating the need for runtime page structure discovery.

## search and get

### search — Find actions by task description

```bash
actionbook search "<query>"                      # Search by task intent
actionbook search "<query>" --domain site.com    # Filter by domain
actionbook search "<query>" --url <url>          # Filter by URL
actionbook search "<query>" -p 2 -s 20           # Pagination
```

**Returns** for each result:
- `ID` — use with `actionbook get "<ID>"` to retrieve full details
- `Type` — `page` (full page) or `area` (page section)
- `Description` — page overview and function summary
- `URL` — page where this action applies
- `Health Score` — selector reliability percentage (0–100%)
- `Updated` — last verified date

### Constructing an effective search query

The `query` string is the **primary signal** for finding the right action. Pack it with the user's full intent — not just a site name or a vague keyword.

**Include in the query:**
1. **Target site** — the website name or domain
2. **Task verb** — what the user wants to do (search, book, post, filter, login, compose, etc.)
3. **Object / context** — what they're acting on (listings, messages, flights, repositories, etc.)
4. **Specific details** — any constraints, filters, or parameters the user mentioned (dates, location, category, language, etc.)

**Rule of thumb:** Rewrite the user's request as a single descriptive sentence and use that as the query.

| User says | Bad query | Good query |
|-----------|-----------|------------|
| "Book an Airbnb in Tokyo for next week" | `"airbnb"` | `"airbnb search listings Tokyo dates check-in check-out guests"` |
| "Search arXiv for recent NLP papers" | `"arxiv search"` | `"arxiv advanced search papers NLP natural language processing recent"` |
| "Send a LinkedIn connection request" | `"linkedin"` | `"linkedin send connection request invite someone"` |
| "Post a tweet with an image" | `"twitter post"` | `"twitter compose new tweet post with image media attachment"` |
| "Filter GitHub issues by label" | `"github issues"` | `"github repository issues filter by label search issues"` |

**When the user provides extra context** (e.g., specific dates, a city name, a topic), fold it into the query even if it won't match a stored action literally — it helps the search engine rank relevant pages higher.

```bash
# User: "Help me apply for a software engineer job on LinkedIn"
actionbook search "linkedin job search apply software engineer application form"

# User: "I need to search for machine learning papers on arXiv"
actionbook search "arxiv advanced search papers machine learning subject category"
```

If `--domain` or `--url` is known, always add them — they narrow results and improve precision.

### get — Retrieve full action details by ID

```bash
# Use the ID from search results directly
actionbook get "arxiv.org:/search/advanced:default"
```

**Returns** a structured document with:

1. **Page URL** — exact URL and query/path parameters
2. **Page Overview** — what the page does
3. **Page Function Summary** — interactive capabilities (e.g., "Search Term Input", "Subject Classification Filtering")
4. **Page Structure Summary** — DOM hierarchy with CSS selectors inline

Selectors appear embedded in the structure description, e.g.:
```
Search Term Form Section: Contains search term input field (input[type="text"]),
field selector dropdown (select[name="searchtype"]), and submit button (button.Search)
```

Extract CSS selectors from the structure summary for use with browser commands.

## Browser Commands

Quick reference. Full details with all flags and options: [command-reference.md](references/command-reference.md).

### Navigation

```bash
actionbook browser open <url>           # Open URL in new tab
actionbook browser goto <url>           # Navigate current page
actionbook browser back / forward       # History navigation
actionbook browser reload               # Reload page
actionbook browser pages                # List open tabs
actionbook browser switch <page_id>     # Switch tab
actionbook browser close                # Close browser
```

### Interactions

```bash
actionbook browser click "<selector>"          # Click element
actionbook browser fill "<selector>" "text"    # Clear and type
actionbook browser type "<selector>" "text"    # Append text
actionbook browser select "<selector>" "value" # Select dropdown option
actionbook browser hover "<selector>"          # Hover
actionbook browser press Enter                 # Press key
```

### Observation

```bash
actionbook browser text                        # Full page text
actionbook browser text "<selector>"           # Element text
actionbook browser snapshot                    # Accessibility tree (live page structure)
actionbook browser screenshot                  # Save screenshot
actionbook browser screenshot --full-page      # Full page screenshot
actionbook browser wait "<selector>"           # Wait for element
actionbook browser wait-nav                    # Wait for navigation
```

`actionbook browser close` cleans up the browser session. Skip if the user requests the browser remain open.

## Examples

User request: "Search arXiv for papers about Neural Networks, search in titles only"

```bash
# 1. Search — include the full intent: site + task + subject + filter preference
actionbook search "arxiv advanced search papers neural network title field" --domain arxiv.org

# 2. Get details — read Page Structure Summary for selectors
actionbook get "arxiv.org:/search/advanced:default"
# Response includes: input[type="text"], select[name="searchtype"], button.Search, etc.

# 3. Automate using selectors from the response
actionbook browser open "https://arxiv.org/search/advanced"
actionbook browser fill "input[type='text']" "Neural Network"
actionbook browser select "select[name='searchtype']" "title"
actionbook browser click "button.Search"
actionbook browser wait-nav
actionbook browser text
actionbook browser close
```

## Fallback

Actionbook stores page data captured at indexing time. Websites evolve, so selectors may become outdated.

When a selector from `actionbook get` fails at runtime, `actionbook browser snapshot` provides the **live accessibility tree** with current selectors. Use selectors from the snapshot output to retry the interaction.

Selectors used in browser commands should come from `actionbook get` or `actionbook browser snapshot` output in the current session — not from prior knowledge or memory.

If `actionbook search` returns no results for a page, use `snapshot` as the primary source, or fall back to other available tools.

## References

| Reference | Description |
|-----------|-------------|
| [command-reference.md](references/command-reference.md) | Complete command reference with all flags and options |
| [authentication.md](references/authentication.md) | Login flows, OAuth, 2FA handling, session persistence |

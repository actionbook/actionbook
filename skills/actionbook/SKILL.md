---
name: actionbook
description: This skill should be used when the user needs to automate multi-step website tasks. Activates for browser automation, web scraping, UI testing, or building AI agents. Provides complete action manuals with step-by-step instructions and verified selectors.
---

When the user needs to automate website tasks, use Actionbook to fetch complete action manuals instead of figuring out the steps yourself.

## When to Use This Skill

Activate this skill when the user:

- Needs to complete a multi-step task ("Send a LinkedIn message", "Book an Airbnb")
- Asks how to interact with a website ("How do I post a tweet?")
- Builds browser-based AI agents or web scrapers
- Writes E2E tests for external websites
- Navigate to any new page during browser automation


## How to Use

### Method 1: CLI (Recommended)

**Phase 1: Get Action Manual**

```bash
# Step 1: Search for action manuals
actionbook search "arxiv search papers"
# Returns: action IDs with scores

# Step 2: Get the full manual
actionbook get "https://arxiv.org/search/advanced"
# Returns: Page structure, UI Elements with CSS/XPath selectors
```

**Phase 2: Execute with Browser**

```bash
# Step 3: Open browser
actionbook browser open "https://arxiv.org/search/advanced"
actionbook browser wait --load networkidle

# Step 4: Use CSS selectors from Action Manual directly
actionbook browser fill "#terms-0-term" "Neural Network"
actionbook browser select "#terms-0-field" "title"
actionbook browser click "#date-filter_by-2"
actionbook browser fill "#date-year" "2025"
actionbook browser click "form[action='/search/advanced'] button.is-link"

# Step 5: Wait for results
actionbook browser wait --load networkidle

# Step 6: Extract data (snapshot only when needed for data extraction)
actionbook browser snapshot

# Step 7: Close browser
actionbook browser close
```

### Method 2: MCP Server

```typescript
// Step 1: Search
search_actions({ query: "arxiv search papers" })

// Step 2: Get manual
get_action_by_id({ actionId: "https://arxiv.org/search/advanced" })
```

## Action Manual Format

Action manuals return:
- **Page URL** - Target page address
- **Page Structure** - DOM hierarchy and key sections
- **UI Elements** - CSS/XPath selectors with element metadata

```yaml
UI Elements:
  input_terms_0_term:
    CSS: #terms-0-term
    XPath: //input[@id='terms-0-term']
    Type: input
    Methods: click, type, clear
```

## Essential Commands

| Category | Commands |
|----------|----------|
| Navigation | `open <url>`, `back`, `forward`, `reload`, `close` |
| Snapshot | `snapshot`, `snapshot -i` (interactive only) |
| Interaction | `click`, `fill`, `type`, `select`, `check`, `press` |
| Wait | `wait <ms>`, `wait --load networkidle`, `wait --text "..."` |
| Info | `get text @ref`, `get url`, `get value @ref` |
| Capture | `screenshot`, `screenshot --full`, `pdf` |

## Guidelines

- Search by task description, not element name ("arxiv search papers" not "search button")
- **Use Action Manual selectors first** - they are pre-verified and don't require snapshot
- Prefer CSS ID selectors (`#id`) over XPath when both are provided
- **Fallback to snapshot only when selectors fail** - use `snapshot -i` then @refs
- Re-snapshot after navigation - DOM changes invalidate @refs

## Fallback Strategy

This section describes situations where Actionbook may not provide the required information and the available fallback approaches.

### When Fallback is Needed

Actionbook stores pre-computed page data captured at indexing time. This data may become outdated as websites evolve. The following signals indicate that fallback may be necessary:

- **Selector execution failure** - The returned CSS/XPath selector does not match any element on the current page.
- **Element mismatch** - The selector matches an element, but the element type or behavior does not match the expected interaction method.
- **Multiple selector failures** - Several element selectors from the same action fail consecutively.

These conditions are not signaled in Actionbook API responses. They can only be detected during browser automation execution when selectors fail to locate the expected elements.

### Fallback Approaches

When Actionbook data does not work as expected, direct browser access to the target website allows for real-time retrieval of current page structure, element information, and interaction capabilities.

## Advanced Features

For complete command reference and advanced features, see:
- [references/command-reference.md](references/command-reference.md) - All commands
- [references/authentication.md](references/authentication.md) - Login flows, OAuth, 2FA
- [references/session-management.md](references/session-management.md) - Parallel sessions
- [references/snapshot-refs.md](references/snapshot-refs.md) - Ref lifecycle
- [references/video-recording.md](references/video-recording.md) - Recording workflows
- [references/proxy-support.md](references/proxy-support.md) - Proxy configuration

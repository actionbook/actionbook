---
name: actionbook
description: Access pre-computed website action manuals containing page descriptions, functionality, DOM structure, and element selectors for browser automation. Use when automating website interactions with Playwright/Puppeteer, need CSS/XPath selectors for UI elements, building browser-based AI agents, or looking up how to interact with a website's UI.
---

# Actionbook

Pre-computed action manuals for browser automation. Agents receive structured page information instead of parsing entire HTML.

## Workflow

1. **search_actions** - Search by keyword, returns URL-based action IDs with content previews
2. **get_action_by_id** - Get full action manual with page details, DOM structure, and element selectors
3. **Execute** - Use returned selectors with Playwright/browser automation

## MCP Tools

- `search_actions` - Search by keyword. Returns: URL-based action IDs, content previews, relevance scores
- `get_action_by_id` - Get full action details. Returns: action content, page element selectors (CSS/XPath), element types, allowed methods (click, type, extract), document metadata

## API

```bash
# Search
curl "https://api.actionbook.dev/api/actions/search?q=airbnb%20search&limit=5"

# Get by ID (URL-based action ID, URL-encoded)
curl "https://api.actionbook.dev/api/actions?id=www.airbnb.com%2Fsearch"
```

### Parameters

**search_actions**:
- `query` (required): Search keyword (e.g., "airbnb search", "google login")
- `type`: `vector` | `fulltext` | `hybrid` (default)
- `limit`: Max results (default: 5)
- `sourceIds`: Filter by source IDs (comma-separated)
- `minScore`: Minimum relevance score (0-1)

**get_action_by_id**:
- `id` (required): URL-based action ID (e.g., `example.com/page`)

## Example Response

```json
{
  "title": "Airbnb Search",
  "url": "www.airbnb.com/search",
  "content": "Search page for Airbnb listings...",
  "elements": [
    {
      "name": "location_input",
      "selector": "input[data-testid='structured-search-input-field-query']",
      "type": "textbox",
      "methods": ["type", "fill"]
    },
    {
      "name": "search_button",
      "selector": "button[data-testid='structured-search-input-search-button']",
      "type": "button",
      "methods": ["click"]
    }
  ]
}
```

## Usage with Playwright

```typescript
// 1. Search for actions
// searchActions("airbnb search") → returns action_id: "https://www.airbnb.com/search"

// 2. Get action details
// getActionById("https://www.airbnb.com/search") → returns selectors

// 3. Use selectors in automation
await page.goto('www.airbnb.com');
await page.fill("input[data-testid='structured-search-input-field-query']", 'Tokyo');
await page.click("button[data-testid='structured-search-input-search-button']");
```

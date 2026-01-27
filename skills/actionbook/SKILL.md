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

## What Actionbook Provides

Action manuals include:

1. **Step-by-step instructions** - The exact sequence to complete a task
2. **Verified selectors** - CSS/XPath selectors for each element
3. **Element metadata** - Type (button, input, etc.) and allowed methods (click, type, fill)

## How to Use

Actionbook can be used in two ways: via MCP (for AI agents) or via CLI (for command line usage).

### Method A: Using MCP (for AI Agents)

**Step 1: Search for Action Manuals**

Call the MCP tool `search_actions` with a task description:

```typescript
// MCP tool call
search_actions({
  query: "linkedin send message"  // or "airbnb book listing", "twitter post tweet"
})
```

**Step 2: Get the Full Manual**

Call the MCP tool `get_action_by_id` with the action ID from search results:

```typescript
// MCP tool call
get_action_by_id({
  actionId: "site/linkedin.com/page/profile/element/message-button"
})
```

### Method B: Using CLI (for Command Line)

**Step 1: Search for Action Manuals**

Use the `actionbook search` command:

```bash
# CLI command
actionbook search "linkedin send message"
actionbook search "airbnb book listing"
actionbook search "twitter post tweet"
```

**Step 2: Get the Full Manual**

Use the `actionbook get` command with the action ID:

```bash
# CLI command
actionbook get "site/linkedin.com/page/profile/element/message-button"
```

### Step 3: Execute the Steps (Both Methods)

Follow the manual steps in order, using the provided selectors.

**Option A: Using agent-browser (via actionbook browser)**

```bash
# Start browser session and navigate
actionbook browser open linkedin.com/in/username

# Click profile elements
actionbook browser click '[data-testid="profile-avatar"]'
actionbook browser click 'button[aria-label="Message"]'

# Fill message and submit
actionbook browser fill 'div[role="textbox"]' 'Hello!'
actionbook browser click 'button[type="submit"]'

# Use following command to get more help
actionbook browser
```

**Option B: Using Playwright/Puppeteer**

```javascript
// LinkedIn send message example
await page.click('[data-testid="profile-avatar"]')
await page.click('button[aria-label="Message"]')
await page.type('div[role="textbox"]', 'Hello!')
await page.click('button[type="submit"]')
```

## Guidelines

- **Search by task**: Describe what you want to accomplish, not just the element (e.g., "linkedin send message" not "linkedin message button")
- **Follow the order**: Execute steps in sequence as provided in the manual
- **Trust the selectors**: Actionbook selectors are verified and maintained

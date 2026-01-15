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

### Step 1: Search for Action Manuals

Call `search_actions` with a task description:

- `query`: "linkedin send message", "airbnb book listing", "twitter post tweet"

### Step 2: Get the Full Manual

Call `get_action_by_id` with the action ID from search results.

### Step 3: Execute the Steps

Follow the manual steps in order, using the provided selectors:

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

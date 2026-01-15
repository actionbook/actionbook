# Actionbook Plugin for Claude Code

Actionbook solves a common problem with browser automation: brittle selectors and wasted tokens parsing HTML. Instead of figuring out page structure each time, Actionbook provides pre-computed action manuals with verified selectors.

## What's Included

This plugin provides:

- **MCP Server** - Connects Claude Code to Actionbook's action manual service
- **Skills** - Auto-triggers when you need to automate website tasks
- **Commands** - `/actionbook:manual` for manual action manual lookups

## Installation

Add the marketplace and install the plugin:

```bash
claude plugin marketplace add actionbook/actionbook
claude plugin install actionbook@actionbook-marketplace
```

## Available Tools

### search_actions

Searches for action manuals matching your task description.

```
Input: "linkedin send message"
Output: [{ id: "linkedin.com/messaging", title: "LinkedIn Send Message", ... }]
```

### get_action_by_id

Fetches the complete action manual with step-by-step instructions and selectors.

```
Input: { id: "linkedin.com/messaging" }
Output: {
  steps: [
    { action: "Click profile avatar", selector: "[data-testid='profile-avatar']", method: "click" },
    { action: "Click message button", selector: "button[aria-label='Message']", method: "click" },
    ...
  ]
}
```

## Usage Examples

The plugin works automatically when you ask about website automation:

- "Send a message on LinkedIn"
- "Book an Airbnb in Tokyo"
- "Post a tweet with an image"
- "Log in to my Google account"

For manual lookups, use the command:

```
/actionbook:manual linkedin send message
/actionbook:manual airbnb search listings
/actionbook:manual twitter post tweet
```

## Why Actionbook?

| Without Actionbook | With Actionbook |
|-------------------|-----------------|
| Parse entire HTML to find elements | Get verified selectors instantly |
| High token costs for DOM context | 100x token savings |
| Selectors break when UI changes | Maintained and updated manuals |
| Guess the steps to complete tasks | Step-by-step instructions |

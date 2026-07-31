---
name: agent-browser
description: Automates browser interactions for form filling and web page interaction. Used by the request-website command to submit website indexing requests.
---

# Browser Automation with agent-browser

## Quick start

```bash
agent-browser open <url>        # Navigate to page
agent-browser snapshot -i       # Get interactive elements with refs
agent-browser click @e1         # Click element by ref
agent-browser fill @e2 "text"   # Fill input by ref
agent-browser close             # Close browser
```

## Core workflow

1. Navigate: `agent-browser open <url>`
2. Snapshot: `agent-browser snapshot -i` (returns elements with refs like `@e1`, `@e2`)
3. Interact using refs from the snapshot
4. Re-snapshot after navigation or significant DOM changes
5. **Always close**: `agent-browser close`

## Commands

### Navigation
```bash
agent-browser open <url>      # Navigate to URL
agent-browser close           # Close browser (ALWAYS do this)
```

### Snapshot (page analysis)
```bash
agent-browser snapshot        # Full accessibility tree
agent-browser snapshot -i     # Interactive elements only (recommended)
```

### Interactions (use @refs from snapshot)
```bash
agent-browser click @e1           # Click
agent-browser fill @e2 "text"     # Clear and type
agent-browser type @e2 "text"     # Type without clearing
agent-browser press Enter         # Press key
agent-browser scroll down 500     # Scroll page
```

### Get information
```bash
agent-browser get text @e1        # Get element text
agent-browser get title           # Get page title
agent-browser get url             # Get current URL
```

### Wait
```bash
agent-browser wait @e1                     # Wait for element
agent-browser wait 2000                    # Wait milliseconds
agent-browser wait --load networkidle      # Wait for network idle
```

## Example: Form submission (request-website)

```bash
# Open Actionbook request page
agent-browser open "https://actionbook.app/request-website"

# Get form elements
agent-browser snapshot -i
# Output shows: textbox "Site URL" [ref=e1], textbox "Email" [ref=e2], textbox "Use Case" [ref=e3], button "Submit" [ref=e4]

# Fill form
agent-browser fill @e1 "https://example.com/products"
agent-browser fill @e2 "user@example.com"
agent-browser fill @e3 "Scraping product catalog"

# Submit
agent-browser click @e4
agent-browser wait --load networkidle

# Verify submission
agent-browser snapshot -i

# Close browser
agent-browser close
```

## Permission Required

Add to `.claude/settings.local.json`:
```json
{
  "permissions": {
    "allow": [
      "Bash(agent-browser *)"
    ]
  }
}
```

Or run `./setup.sh` to configure automatically.

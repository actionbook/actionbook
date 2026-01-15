---
description: Get action manual for a website task
argument-hint: <task>
---

# /actionbook:manual

Fetches up-to-date action manuals with step-by-step instructions and verified selectors.

## Usage

```
/actionbook:manual <task>
```

- **task**: Describe what you want to do (e.g., "linkedin send message", "airbnb book listing")

## Examples

```
/actionbook:manual linkedin send message
/actionbook:manual airbnb search listings
/actionbook:manual twitter post tweet
/actionbook:manual google login
/actionbook:manual github create issue
```

## How It Works

1. Searches for action manuals matching your task
2. Returns the best matching manual with:
   - Step-by-step instructions
   - Verified CSS/XPath selectors for each element
   - Element types and allowed methods (click, type, fill, etc.)
3. You can use these selectors directly with Playwright, Puppeteer, or any browser automation tool

# Generate Scraper Script with Claude Code + Actionbook

This demo shows how **Actionbook** helps AI Agents (like Claude Code) generate more accurate web scraping scripts by providing verified selectors and page structure information.

## Demo

Watch how Actionbook helps Claude Code write a correct scraper, 10x faster.

https://github.com/user-attachments/assets/912b7d39-1e55-43b7-b766-344b242762e9

## The Problem

When you ask Claude Code to "write a scraper for X website", it needs to figure out:

- What CSS selectors to use
- How the page structure works
- How to handle dynamic content
- Which elements contain the data you need

**Without Actionbook**, Claude Code has to guess selectors based on common patterns, often resulting in:

- Wrong or fragile selectors
- Missing data fields
- Scripts that break when the page structure changes
- Multiple rounds of trial and error

**With Actionbook**, Claude Code gets verified selectors and page structure directly, generating working scripts on the first try.

## Quick Start

### 1. Install Actionbook

**Install via plugin**

```sh
claude plugin marketplace add actionbook/actionbook
claude plugin install actionbook@actionbook-marketplace
```

**Install manually**

```sh
claude mcp add actionbook -- npx -y @actionbookdev/mcp@latest
npx skills add actionbook/actionbook
```

### 2. Add to Your Prompt

When asking Claude Code to write a scraper, add this to your prompt:

```
Use Actionbook to understand the page before taking action.
```

That's it! Claude Code will automatically query Actionbook for verified selectors and page structure.

## Comparison

### Without Actionbook

See [comparasion/cc-without-actionbook/](./comparasion/cc-without-actionbook/)

- Claude Code **guessed** selectors like `.company-list-card-medium-large`
- Had to visit each company's detail page individually
- Took **multiple attempts** to find working selectors
- Result: Only 25 companies scraped, some with incomplete data

### With Actionbook

See [comparasion/cc-with-actionbook/](./comparasion/cc-with-actionbook/)

- Claude Code got **verified selectors** from Actionbook
- Knew exactly how to expand cards and extract details
- Generated working script on **first attempt**
- Result: All 194 companies scraped with complete data

# Actionbook Scraper Plugin

Generate accurate web scraper code using Actionbook's verified selectors.

## The Problem

When AI agents try to generate web scraper code, they face a fundamental challenge: **guessing CSS selectors**. Even the most advanced models struggle with:

- Complex, dynamically-generated class names
- Nested component structures
- State-dependent elements (expanded vs collapsed)
- Lazy-loaded content

This leads to scrapers that either fail completely or extract incomplete data.

## The Solution: Actionbook

Actionbook provides AI agents with **verified, curated selectors** for websites. Instead of guessing, agents get:

- Exact CSS selectors that work
- XPath alternatives for complex cases
- Page structure understanding
- Interaction patterns (scroll, click, wait)

## Auto-Verification (Two-Part Check)

**Every generated script is automatically verified with two checks:**

| Check | What's Verified |
|-------|-----------------|
| **1. Script Runs** | No errors, no timeouts |
| **2. Data Correct** | Content matches expected fields |

```
Generate Script → Execute → Check Script Runs → Check Data Correct
                                                        ↓
                              Both Pass: Output script + validated data preview
                              Either Fails: Analyze → Fix → Retry (max 3x)
```

**Why Part 2 matters:**

A script can run successfully but extract wrong data:
- Button text like "Click to expand" instead of company name
- Placeholder text like "Loading..." instead of description
- Fields mapped incorrectly (year contains location)

The verification catches these issues and fixes them automatically.

## Actionbook Issues Logging

**If Actionbook selectors are wrong or outdated, issues are logged locally:**

```
.actionbook-issues.log
```

**Issue types recorded:**

| Type | Description |
|------|-------------|
| `selector_error` | Selector doesn't find any element |
| `outdated` | Selector finds wrong element (page changed) |
| `missing` | Needed selector not in Actionbook |
| `incorrect` | Selector metadata is wrong |

**Log format:**
```
[2024-01-15 14:30] URL: https://example.com/companies
Action ID: https://example.com/companies
Issue Type: selector_error
Details: Card selector no longer exists after site redesign
Selector: .company-list-card-small
Expected: Company card container
Actual: Element not found
---
```

See `.actionbook-issues.log.example` for more examples.

## Demo Results: 7.76x Improvement

We tested scraper generation on [First Round Capital's portfolio page](https://firstround.com/companies):

| Metric | Without Actionbook | With Actionbook |
|--------|-------------------|-----------------|
| Companies Extracted | 25 | **194** |
| Data Completeness | Partial | **Full** |
| Selector Accuracy | ~13% | **100%** |
| Iterations Needed | Multiple | **1** |

### Without Actionbook

The AI guessed selectors like `.company-list-card-medium-large`, which don't exist. After multiple attempts and corrections, it managed to extract only 25 companies with incomplete data.

### With Actionbook

The AI received verified selectors immediately:
- Card container: `.company-list-card-small`
- Company name: `.company-list-card-small__button-name`
- Expand button: `button.company-list-card-small__button`
- Detail items: `.company-list-company-info__item` (dt/dd pairs)

First attempt: **194 companies with complete data**.

## Installation

### Step 1: Add Marketplace

```bash
claude plugin marketplace add actionbook/actionbook
```

### Step 2: Install Plugin

```bash
claude plugin install actionbook-scraper@actionbook-marketplace
```

### Step 3: Configure Permissions

The `request-website` command uses `agent-browser` for form automation. Configure permissions using one of these methods:

**Option A: Run setup script (recommended)**
```bash
./setup.sh
```

**Option B: Manual configuration**

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

**Option C: Use /permissions command**
```
/permissions allow Bash(agent-browser *)
```

### Step 4: Restart Claude

Restart Claude Code to apply the permission changes.

### Manual Installation (Alternative)

1. Clone or copy this directory to your project
2. The `.mcp.json` configures Actionbook MCP automatically
3. Run `./setup.sh` to configure permissions

## Commands

| Command | Description |
|---------|-------------|
| `/actionbook-scraper:list-sources` | List websites with Actionbook data |
| `/actionbook-scraper:analyze <url>` | Analyze page structure and selectors |
| `/actionbook-scraper:generate <url>` | **Interactive**: Scrape data now with agent-browser |
| `/actionbook-scraper:generate <url> --standalone` | **Standalone**: Generate Playwright script |
| `/actionbook-scraper:request-website <url>` | Request a new website to be indexed |

## Two Script Types

**This plugin generates scraper scripts - it does NOT execute scraping.**

### Default: Generate agent-browser Script

```
/actionbook-scraper:generate https://firstround.com/companies
```

**Output:** agent-browser commands you can run

```bash
agent-browser open "https://firstround.com/companies"
agent-browser scroll down 2000
agent-browser get text ".company-list-card-small"
agent-browser close
```

### Standalone: Generate Playwright Script

```
/actionbook-scraper:generate https://firstround.com/companies --standalone
```

**Output:** Playwright/Puppeteer JavaScript code

```javascript
const { chromium } = require('playwright');
// ... full script
```

### Comparison

| Mode | Output | Run With |
|------|--------|----------|
| Default | agent-browser commands | `agent-browser` CLI |
| --standalone | Playwright .js file | `node scraper.js` |

## Quick Start

### 1. Check Available Sources

```
/actionbook-scraper:list-sources
```

### 2. Analyze a Page

```
/actionbook-scraper:analyze https://firstround.com/companies
```

Output shows:
- Available selectors
- Page structure (static/dynamic)
- Recommended scraping approach

### 3. Generate Scraper Script

```
/actionbook-scraper:generate https://firstround.com/companies
```

Output: agent-browser script with verified selectors

```bash
agent-browser open "https://firstround.com/companies"
agent-browser scroll down 2000
agent-browser get text ".company-list-card-small"
agent-browser close
```

Run the generated commands to scrape the data.

## Usage Examples

### Example 1: Generate agent-browser Script (Default)

```
/actionbook-scraper:generate https://firstround.com/companies
```

Output: agent-browser commands with Actionbook selectors
```bash
agent-browser open "https://firstround.com/companies"
agent-browser scroll down 2000
agent-browser get text ".company-list-card-small"
agent-browser close
```

### Example 2: Generate Playwright Script

```
/actionbook-scraper:generate https://firstround.com/companies --standalone
```

Output: Playwright JavaScript code to save as `scraper.js`

### Example 3: Generate Python Script

```
/actionbook-scraper:generate https://example.com/products --standalone --template playwright-python
```

Output: Python Playwright code to save as `scraper.py`

### Example 4: Analyze Page Structure

```
/actionbook-scraper:analyze https://example.com/listings
```

Output: Available selectors and page structure analysis

## Modes & Templates

| Mode | Template | Best For |
|------|----------|----------|
| **Interactive** (default) | agent-browser | One-time extraction, quick data grabs |
| Standalone | playwright-js | Scheduled tasks, deployment |
| Standalone | playwright-python | Python/data science workflows |
| Standalone | puppeteer | Static pages, simple extraction |

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                       User Commands                               │
│   /analyze   /generate   /list-sources   /request-website        │
└──────────────────────────────┬───────────────────────────────────┘
                               │
                               ▼
┌──────────────────────────────────────────────────────────────────┐
│                          Agents                                   │
│  ┌─────────────┐  ┌────────────────────┐  ┌──────────────────┐   │
│  │  structure- │  │     /generate      │  │website-requester │   │
│  │  analyzer   │  ├────────────────────┤  │                  │   │
│  │   (haiku)   │  │ Interactive│Standa │  │    (haiku)       │   │
│  │             │  │   (haiku)  │(sonnet│  │                  │   │
│  └─────────────┘  └────────────────────┘  └──────────────────┘   │
└──────────────────────────────┬───────────────────────────────────┘
                               │
┌──────────────────────────────▼───────────────────────────────────┐
│                        agent-browser                              │
│            (scraping / form submission / page interaction)        │
└──────────────────────────────┬───────────────────────────────────┘
                               │
┌──────────────────────────────▼───────────────────────────────────┐
│                        Actionbook MCP                             │
│          search_actions / get_action_by_id / list_sources        │
└──────────────────────────────────────────────────────────────────┘
```

## File Structure

```
actionbook-scraper/
├── .claude-plugin/
│   ├── plugin.json         # Plugin configuration
│   └── marketplace.json    # Marketplace listing
├── .mcp.json               # MCP server configuration
├── settings.example.json   # Permission template
├── setup.sh                # Setup script
├── skills/
│   ├── actionbook-scraper/
│   │   └── SKILL.md        # Master skill reference
│   └── agent-browser/
│       └── SKILL.md        # Browser automation skill
├── commands/
│   ├── analyze.md          # Analyze command
│   ├── generate.md         # Generate command (interactive + standalone)
│   ├── list-sources.md     # List sources command
│   └── request-website.md  # Request new website command
├── agents/
│   ├── structure-analyzer.md  # Haiku agent for analysis
│   ├── scraper-executor.md    # Haiku agent for interactive scraping
│   ├── code-generator.md      # Sonnet agent for standalone scripts
│   └── website-requester.md   # Haiku agent for form submission
├── templates/
│   ├── agent-browser.md    # Interactive scraping template
│   ├── playwright-js.md    # Standalone: JavaScript + Playwright
│   ├── playwright-python.md # Standalone: Python + Playwright
│   └── puppeteer.md        # Standalone: JavaScript + Puppeteer
└── README.md
```

## Requesting New Websites

If a website isn't indexed in Actionbook, you can request it:

```
/actionbook-scraper:request-website https://example.com/page
```

With optional parameters:
```
/actionbook-scraper:request-website https://example.com/page --email you@email.com --use-case "scraping product data"
```

This uses `agent-browser` to submit a request form at [actionbook.app/request-website](https://actionbook.app/request-website). Actionbook prioritizes indexing based on user demand.

## Troubleshooting

### "No selectors found for URL"

The website may not be indexed. Options:
1. Run `/actionbook-scraper:list-sources` to see available sites
2. Run `/actionbook-scraper:request-website <url>` to request indexing

### "Partial data extracted"

Some selectors may be outdated. Try:
1. Run `/actionbook-scraper:analyze <url>` to check current selectors
2. Verify the page structure hasn't changed
3. Report outdated selectors to Actionbook

### Generated script times out

Increase timeout values in the generated script or add more specific wait conditions.

## Contributing

Found a website that needs better selectors? Contributions to Actionbook's selector database are welcome!

## License

MIT

## Related

- [Actionbook](https://github.com/actionbook/actionbook) - Website Action Service Platform
- [Generate Scraper Demo](../generate-scraper-script/) - Original demo showing the comparison

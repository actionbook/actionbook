# Actionbook Dify Plugin

Access verified website selectors and operation manuals directly from your Dify workflows and agents.

## Features

- 🔍 **Search Actions**: Find website elements by keyword or context
- 📋 **Get Action Details**: Retrieve complete selector information and allowed methods
- ✅ **Verified Selectors**: All selectors are tested and maintained by the Actionbook community
- 🚀 **No Browser Required**: Query manuals without launching browsers (Phase 1)

## Installation

1. Visit [Dify Marketplace](https://marketplace.dify.ai)
2. Search for "Actionbook"
3. Click **Install**
4. Enter your Actionbook API Key

**Get API Key**: Sign up at [actionbook.dev](https://actionbook.dev) and visit your [Dashboard → API Keys](https://actionbook.dev/dashboard/api-keys)

## Tools

### search_actions

Search for website actions by keyword or context.

**Parameters**:
- `query` (required): Keyword describing the action (e.g., "login button")
- `domain` (optional): Filter by website domain (e.g., "github.com")
- `limit` (optional): Max results (1-50, default: 10)

**Example Usage**:
```
Query: "GitHub login form"
Domain: "github.com"
Limit: 5
```

**Returns**:
```
Area ID: github.com:login:username-field
Description: Username or email input field
Health Score: 95/100
Selectors: #login_field, input[name="login"]
---
Area ID: github.com:login:password-field
...
```

### get_action_by_area_id

Get full details for a specific action.

**Parameters**:
- `area_id` (required): Area ID from search results (format: `site:path:area`)

**Example Usage**:
```
Area ID: github.com:login:username-field
```

**Returns**:
```
Site: github.com
Page: /login
Area: username-field

Element: email-input
Selectors:
  - CSS: #login_field
  - XPath: //input[@name='login']
  - Aria Label: Username or email address

Allowed Methods: click, type, clear
Last Verified: 2026-02-05
```

## Use Cases

### 1. Web Scraper Builder
```
Workflow:
1. Use search_actions to find product listing elements
2. Use get_action_by_area_id to get exact selectors
3. Pass selectors to scraping tool (Phase 2: browser automation)
```

### 2. Automated Testing
```
Agent Flow:
1. Search for "submit button on checkout page"
2. Get action details for verification
3. Generate test cases with verified selectors
```

### 3. Research Assistant
```
Multi-Agent:
1. Agent A: Use search_actions to find arXiv search form
2. Agent B: Use selectors to build query
3. Agent C: Extract and summarize papers
```

## Agent Configuration

When using Actionbook tools with a **Chatbot + Agent** mode application in Dify:

### Recommended Settings

- **Agent Strategy**: Function Calling (preferred) or ReAct
- **Model**: GPT-4 / Claude 3.5+ (must support Function Calling)
- **Maximum Iterations**: 5+ (set to at least 5 for chained tool calls; setting to 1 prevents tool invocation)

### System Prompt Example

Include in your agent's system prompt:

```
You have access to Actionbook tools for browser automation.
When a user asks about website elements, selectors, or browser automation:
1. Use search_actions to find relevant website selectors by keyword
2. Use get_action_by_area_id with an area_id from the search results to get complete selector details
```

### Troubleshooting: Agent Not Calling Tools

If the Agent replies directly without invoking tools:

1. **Check Agent Strategy**: Must be "Function Calling" or "ReAct" (not basic chat)
2. **Check Model**: Must support Function Calling (e.g., GPT-4, Claude 3.5+)
3. **Check Maximum Iterations**: Must be > 1 (recommended: 5+)
4. **Add System Prompt**: Explicitly instruct the agent to use Actionbook tools for automation queries

## Roadmap

**Phase 1** (Current): Query action manuals via API
**Phase 2** (Coming Soon): Remote browser control via CDP
- Connect to Browserbase / Browser.cloud
- Fine-grained operations: click, fill, goto, snapshot, test

## Support

- Documentation: [docs.actionbook.dev](https://docs.actionbook.dev)
- GitHub: [github.com/actionbook/actionbook](https://github.com/actionbook/actionbook)
- Issues: [Linear](https://linear.app/cue-labs)

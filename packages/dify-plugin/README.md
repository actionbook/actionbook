# Actionbook Dify Plugin

Access verified website selectors and operation manuals directly from your Dify workflows and agents — with built-in cloud browser automation.

## Features

- **Search Actions**: Find website elements by keyword or context
- **Get Action Details**: Retrieve complete selector information and allowed methods
- **Verified Selectors**: All selectors are tested and maintained by the Actionbook community
- **Cloud Browser Sessions**: Create and manage cloud browser sessions via Hyperbrowser
- **Browser Operator**: Navigate, click, fill, snapshot and more — all from Dify workflows

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

### browser_create_session

Start a cloud browser session via a managed provider (Hyperbrowser).

**Parameters**:
- `provider` (optional, form): Cloud browser provider. Default: `hyperbrowser`
- `api_key` (required, form): Provider API key (stored as secret)
- `profile_id` (optional): Stable identifier for browser state persistence across sessions
- `use_proxy` (optional, form): Route through a residential proxy. Default: `false`

**Returns**: `session_id` and `ws_endpoint` (WebSocket CDP URL) for use with `browser_operator`.

### browser_stop_session

Stop a cloud browser session and release resources.

**Parameters**:
- `session_id` (required): Session ID from `browser_create_session`

Provider and API key are automatically resolved from the session created by `browser_create_session` — no need to provide them again.

### browser_operator

Unified browser operator for all page interactions.

**Parameters**:
- `session_id` (optional): Session ID from `browser_create_session` (preferred for multi-step workflows)
- `cdp_url` (optional): WebSocket CDP URL as fallback
- `action` (required): One of: `navigate`, `click`, `type`, `fill`, `select`, `press_key`, `hover`, `snapshot`, `get_text`, `get_html`, `wait`, `wait_navigation`, `go_back`, `go_forward`, `reload`
- `url` (optional): Target URL for `navigate`
- `selector` (optional): CSS selector for element-targeting actions
- `text` (optional): Text content for `type`/`fill`
- `value` (optional): Option value for `select`
- `key` (optional): Key name for `press_key`
- `timeout_ms` (optional): Timeout in milliseconds (default: 30000)

For best reliability, pass **both** `session_id` and `cdp_url` on each call.

## Use Cases

### 1. End-to-End Browser Automation
```
Workflow:
1. search_actions -> find selectors for the target page
2. browser_create_session -> get session_id + ws_endpoint
3. browser_operator(navigate) -> go to the target page
4. browser_operator(fill/click) -> interact using verified selectors
5. browser_operator(snapshot) -> inspect page state if needed
6. browser_stop_session -> release resources
```

### 2. Automated Testing
```
Agent Flow:
1. Search for "submit button on checkout page"
2. Get action details with verified selectors
3. Create browser session and execute test steps
4. Stop session and report results
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
You can use Actionbook tools.
Workflow:
1) search_actions(query, domain?) -> pick best area_id
2) get_action_by_area_id(area_id)
3) browser_create_session(api_key) -> store session_id and ws_endpoint
4) For EVERY browser_operator call, pass BOTH:
   - session_id = from create_session
   - cdp_url = ws_endpoint from create_session
5) If click/fill/type fails with Element not found or Timeout:
   - call browser_operator(action="snapshot")
   - derive a new selector from snapshot and retry once
6) Only call browser_stop_session(session_id) after task is done or hard failure.
   Provider and API key are auto-resolved — just pass session_id.
```

### Troubleshooting: Agent Not Calling Tools

If the Agent replies directly without invoking tools:

1. **Check Agent Strategy**: Must be "Function Calling" or "ReAct" (not basic chat)
2. **Check Model**: Must support Function Calling (e.g., GPT-4, Claude 3.5+)
3. **Check Maximum Iterations**: Must be > 1 (recommended: 5+)
4. **Add System Prompt**: Explicitly instruct the agent to use Actionbook tools for automation queries

### Troubleshooting: Session Created But operator Fails

If you see:
- `Error: No pooled connection for session ...`

Then:
1. Ensure each `browser_operator` call includes `cdp_url=ws_endpoint` from `browser_create_session`.
2. Keep `session_id` too (send both `session_id + cdp_url`).
3. Ensure previous runs call `browser_stop_session`; otherwise provider may return:
   `Maximum number of active sessions ... reached`.

## Support

- Documentation: [docs.actionbook.dev](https://docs.actionbook.dev)
- GitHub: [github.com/actionbook/actionbook](https://github.com/actionbook/actionbook)
- Issues: [Linear](https://linear.app/cue-labs)

# @actionbookdev/cli

CLI for Actionbook - Get website action manuals for AI agents.

## Installation

```bash
npm install -g @actionbookdev/cli
```

## Quick Start

```bash
# Search for actions
actionbook search "airbnb search"

# Get action details
actionbook get "https://www.airbnb.com/search"

# List available sources
actionbook sources

# Search sources
actionbook sources search "linkedin"

# Browser automation (requires agent-browser)
actionbook browser open example.com
actionbook browser snapshot -i
```

## Commands

### `actionbook search <query>`

Search for action manuals by keyword.

```bash
actionbook search "google login"
actionbook search "airbnb" --type vector --limit 10
actionbook search "login" --source-ids 1,2,3
```

**Options:**
- `-t, --type <type>` - Search type: `vector`, `fulltext`, or `hybrid` (default: `hybrid`)
- `-l, --limit <number>` - Maximum results 1-100 (default: `5`)
- `-s, --source-ids <ids>` - Filter by source IDs (comma-separated)
- `--min-score <score>` - Minimum similarity score 0-1
- `-j, --json` - Output raw JSON

**Alias:** `actionbook s`

### `actionbook get <id>`

Get complete action details by action ID.

```bash
actionbook get "https://www.airbnb.com/search"
actionbook get "airbnb.com/search"  # fuzzy matching supported
actionbook get "releases.rs"        # domain only
```

**Options:**
- `-j, --json` - Output raw JSON

**Alias:** `actionbook g`

### `actionbook sources`

List all available sources (websites).

```bash
actionbook sources
actionbook sources --limit 100
actionbook sources --json
```

**Options:**
- `-l, --limit <number>` - Maximum results (default: `50`)
- `-j, --json` - Output raw JSON

### `actionbook sources search <query>`

Search for sources by keyword.

```bash
actionbook sources search "airbnb"
actionbook sources search "e-commerce" --limit 20
```

**Options:**
- `-l, --limit <number>` - Maximum results (default: `10`)
- `-j, --json` - Output raw JSON

**Alias:** `actionbook sources s`

### `actionbook browser [command]`

Execute browser automation commands. This command forwards all arguments to `agent-browser` CLI.

```bash
# Open a website
actionbook browser open example.com

# Take interactive snapshot
actionbook browser snapshot -i

# Click element by reference
actionbook browser click @e1

# Fill input field
actionbook browser fill @e3 "test@example.com"

# Check current session
actionbook browser session

# Close browser
actionbook browser close
```

**Prerequisites:**

Install `agent-browser` globally:

```bash
npm install -g agent-browser
```

**Common Commands:**
- `open <url>` - Navigate to URL
- `snapshot -i` - Get interactive elements with references
- `click <selector>` - Click element (or @ref)
- `fill <selector> <text>` - Fill input field
- `type <selector> <text>` - Type into element
- `wait <selector|ms>` - Wait for element or time
- `screenshot [path]` - Take screenshot
- `close` - Close browser

**For full command list:**

```bash
actionbook browser --help
actionbook browser  # Shows full agent-browser help
```

**Learn more:** [agent-browser on GitHub](https://github.com/vercel-labs/agent-browser)

## Authentication

Set your API key via environment variable:

```bash
export ACTIONBOOK_API_KEY=your_api_key
```

Or pass it as an option:

```bash
actionbook --api-key your_api_key search "query"
```

## Output Formats

By default, the CLI outputs formatted, colorized results for human readability.

Use `--json` flag for raw JSON output, useful for piping to other tools:

```bash
actionbook search "login" --json | jq '.results[0].action_id'
```

## Examples

### Typical Workflow

```bash
# 1. Search for actions
actionbook search "airbnb search"

# 2. Get details for a specific action
actionbook get "https://www.airbnb.com/search"

# 3. Use the selectors in your automation script
```

### Filter by Source

```bash
# List sources to find IDs
actionbook sources

# Search within specific sources
actionbook search "login" --source-ids 1,2
```

### JSON Output for Scripts

```bash
# Get action and extract selectors
actionbook get "booking.com" --json | jq '.elements'
```

### Browser Automation Workflow

```bash
# 1. Get action details with verified selectors
actionbook get "github.com/login"

# 2. Use browser command to automate
actionbook browser open "https://github.com/login"
actionbook browser snapshot -i
actionbook browser fill @e1 "username"
actionbook browser fill @e2 "password"
actionbook browser click @e3
```

## Related Packages

- [`@actionbookdev/sdk`](https://www.npmjs.com/package/@actionbookdev/sdk) - JavaScript/TypeScript SDK
- [`@actionbookdev/mcp`](https://www.npmjs.com/package/@actionbookdev/mcp) - MCP Server for AI agents

## License

MIT

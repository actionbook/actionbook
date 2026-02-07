# @actionbookdev/cli

CLI for Actionbook - Browser automation and action manuals for AI agents. Powered by a native Rust binary for fast startup and zero runtime dependencies.

## Installation

```bash
npm install -g @actionbookdev/cli
```

Or use directly with npx:

```bash
npx @actionbookdev/cli search "airbnb search"
```

## Platform Binaries

`@actionbookdev/cli` is the single public install package.
Platform-specific native binaries are shipped through internal optional dependencies
(`@actionbookdev/cli-*`), and npm automatically installs the matching package for
your OS/CPU.

If you install with `--omit=optional`, the native binary package may be skipped and
the CLI will not run until you reinstall without that flag.

## Quick Start

```bash
# Search for actions
actionbook search "airbnb search"

# Get action details by area_id
actionbook get "airbnb.com:/:default"

# Browser automation
actionbook browser open https://example.com
actionbook browser snapshot
actionbook browser click "button.submit"
```

## Commands

### `actionbook search <query>`

Search for action manuals by keyword.

```bash
actionbook search "google login"
actionbook search "airbnb" --domain airbnb.com
actionbook search "login" --page 2 --page-size 20
```

**Options:**
- `-d, --domain <domain>` - Filter by domain (e.g., "airbnb.com")
- `-u, --url <url>` - Filter by URL
- `-p, --page <number>` - Page number (default: `1`)
- `-s, --page-size <number>` - Results per page 1-100 (default: `10`)

### `actionbook get <area_id>`

Get complete action details by area ID.

```bash
actionbook get "airbnb.com:/:default"
actionbook get "github.com:/login:default"
```

### `actionbook browser <command>`

Browser automation via Chrome DevTools Protocol. Uses your existing system browser (Chrome, Brave, Edge, Arc, Chromium) - no browser download required.

**Navigation:**
- `open <url>` - Open URL in new tab
- `goto <url>` - Navigate current page
- `back` / `forward` / `reload` - History navigation
- `pages` / `switch` - Manage tabs

**Interaction:**
- `click <selector>` - Click element
- `type <selector> <text>` - Type text (append)
- `fill <selector> <text>` - Clear and type text
- `select <selector> <value>` - Select dropdown option
- `hover <selector>` / `focus <selector>` - Hover/focus element
- `press <key>` - Press keyboard key

**Waiting:**
- `wait <selector>` - Wait for element (default 30s)
- `wait-nav` - Wait for navigation

**Page Inspection:**
- `screenshot [path]` - Take screenshot
- `pdf <path>` - Export as PDF
- `html [selector]` - Get page/element HTML
- `text [selector]` - Get page/element text
- `eval <code>` - Execute JavaScript
- `snapshot` - Get accessibility snapshot
- `viewport` - Get viewport dimensions

**Cookies:**
- `cookies list` / `get` / `set` / `delete` / `clear` - Cookie management

**Session:**
- `status` - Show detected browsers & session info
- `close` / `restart` / `connect` - Session control

### `actionbook sources`

List and search available action sources.

```bash
actionbook sources list
actionbook sources search "github"
```

### `actionbook config`

Manage CLI configuration.

```bash
actionbook config show
actionbook config get api.base_url
actionbook config set api.api_key "your_key"
```

### `actionbook profile`

Manage browser profiles for isolated sessions.

```bash
actionbook profile list
actionbook profile create work
actionbook profile delete work
```

## Global Options

```bash
--browser-path <path>    # Custom browser executable
--cdp <port|url>         # Connect to existing CDP port
--profile <name>         # Use specific browser profile
--headless               # Run in headless mode
--json                   # JSON output format
--verbose, -v            # Verbose logging
```

## Configuration

Config file location: `~/.config/actionbook/config.toml`

```toml
[api]
base_url = "https://api.actionbook.dev"
api_key = "your_key"

[browser]
executable = "/path/to/chrome"
default_profile = "default"
headless = false
```

**Priority:** CLI args > Environment vars > Config file > Auto-discovery

## Environment Variables

- `ACTIONBOOK_API_KEY` - API key for Actionbook service
- `ACTIONBOOK_BINARY_PATH` - Override binary path (for development)

## Supported Browsers

| Browser | macOS | Linux | Windows |
|---------|-------|-------|---------|
| Google Chrome | Yes | Yes | Yes |
| Brave | Yes | Yes | Yes |
| Microsoft Edge | Yes | Yes | Yes |
| Arc | Yes | - | - |
| Chromium | Yes | Yes | Yes |

## Related Packages

- [`@actionbookdev/sdk`](https://www.npmjs.com/package/@actionbookdev/sdk) - JavaScript/TypeScript SDK
- [`@actionbookdev/mcp`](https://www.npmjs.com/package/@actionbookdev/mcp) - MCP Server for AI agents
- [`@actionbookdev/tools-ai-sdk`](https://www.npmjs.com/package/@actionbookdev/tools-ai-sdk) - Vercel AI SDK tools

## License

Apache-2.0

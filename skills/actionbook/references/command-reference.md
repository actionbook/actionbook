# actionbook Command Reference

Complete reference for all `actionbook` CLI commands.

## Action Search & Retrieval

### search - Search for action manuals

```bash
actionbook search "<query>"                    # Basic keyword search
actionbook search "<query>" --domain site.com  # Filter by domain
actionbook search "<query>" --url <url>        # Filter by specific URL
actionbook search "<query>" -p 2               # Page 2 (default: 1)
actionbook search "<query>" -s 20              # 20 results per page (default: 10, max: 100)
actionbook search "<query>" --domain site.com --url <url> -p 1 -s 5  # Combined
```

Returns area IDs with descriptions and relevance scores. Use the area_id with `actionbook get` to fetch full details.

### get - Get action details by area ID

```bash
actionbook get "<area_id>"
# area_id format: "site.com:/path:area_name"
# Examples:
#   actionbook get "airbnb.com:/:default"
#   actionbook get "github.com:/login:form"
#   actionbook get "arxiv.org:/search/advanced:default"
```

Returns complete action manual with CSS/XPath selectors, element types, and allowed methods.

### sources - List and search sources

```bash
actionbook sources list                        # List all available sources
actionbook sources search "<query>"            # Search sources by keyword
```

## Browser Automation

### Navigation

```bash
actionbook browser open <url>                  # Open URL in new tab
actionbook browser goto <url>                  # Navigate current page to URL
actionbook browser goto <url> --timeout 5000   # Custom navigation timeout (default: 30000ms)
actionbook browser back                        # Go back in history
actionbook browser forward                     # Go forward in history
actionbook browser reload                      # Reload current page
actionbook browser pages                       # List all open pages/tabs
actionbook browser switch <page_id>            # Switch to specific page by ID
actionbook browser close                       # Close the browser
actionbook browser restart                     # Restart the browser
actionbook browser connect <endpoint>          # Connect to existing browser (CDP port or ws:// URL)
actionbook browser status                      # Show detected browsers and session status
```

### Element Interactions

All interaction commands take CSS selectors. Use selectors from Action Manuals or from `snapshot` output.

```bash
actionbook browser click "<selector>"                  # Click element
actionbook browser click "<selector>" --wait 1000      # Wait 1s for element, then click
actionbook browser fill "<selector>" "text"            # Clear element, then type text
actionbook browser fill "<selector>" "text" --wait 500 # Wait for element, then fill
actionbook browser type "<selector>" "text"            # Type text (append, no clear)
actionbook browser type "<selector>" "text" --wait 500 # Wait for element, then type
actionbook browser select "<selector>" "value"         # Select dropdown option by value
actionbook browser hover "<selector>"                  # Hover over element
actionbook browser focus "<selector>"                  # Focus on element
actionbook browser press <key>                         # Press keyboard key
# Key examples: Enter, Tab, Escape, ArrowDown, ArrowUp, Backspace, Delete, Space
```

### Information Retrieval

```bash
actionbook browser text                        # Get full page text content
actionbook browser text "<selector>"           # Get text of specific element
actionbook browser html                        # Get full page HTML
actionbook browser html "<selector>"           # Get outer HTML of specific element
actionbook browser snapshot                    # Get accessibility snapshot (tree structure)
actionbook browser viewport                    # Get viewport dimensions
```

### Wait Conditions

```bash
actionbook browser wait "<selector>"                   # Wait for element to appear
actionbook browser wait "<selector>" --timeout 5000    # Custom timeout (default: 30000ms)
actionbook browser wait-nav                            # Wait for navigation to complete
actionbook browser wait-nav --timeout 10000            # Custom navigation timeout
```

### Screenshots & Export

```bash
actionbook browser screenshot                  # Save as screenshot.png
actionbook browser screenshot output.png       # Save to custom path
actionbook browser screenshot --full-page      # Full page screenshot
actionbook browser pdf output.pdf              # Export page as PDF
```

### JavaScript & DOM Inspection

```bash
actionbook browser eval "document.title"               # Execute JavaScript, return result
actionbook browser eval "document.querySelectorAll('a').length"
actionbook browser inspect <x> <y>                     # Inspect DOM element at viewport coordinates
actionbook browser inspect <x> <y> --desc "login btn"  # With description hint
```

### Scrolling

```bash
actionbook browser scroll down                 # Scroll down one viewport height
actionbook browser scroll down 500             # Scroll down 500px
actionbook browser scroll up                   # Scroll up one viewport height
actionbook browser scroll up 300               # Scroll up 300px
actionbook browser scroll bottom               # Scroll to page bottom
actionbook browser scroll top                  # Scroll to page top
actionbook browser scroll to "<selector>"      # Scroll element into view (centered)
actionbook browser scroll to "<selector>" --align start  # Align: start, center, end, nearest
actionbook browser scroll down --smooth        # Smooth scrolling (works with all directions)
```

### Cookie Management

```bash
actionbook browser cookies                     # List all cookies (default)
actionbook browser cookies list                # List all cookies
actionbook browser cookies get "<name>"        # Get specific cookie by name
actionbook browser cookies set "<name>" "<value>"              # Set cookie
actionbook browser cookies set "<name>" "<value>" --domain ".example.com"  # Set with domain
actionbook browser cookies delete "<name>"     # Delete specific cookie
actionbook browser cookies clear               # Clear all cookies for current page
actionbook browser cookies clear --domain ".example.com"       # Clear cookies for specific domain
actionbook browser cookies clear --dry-run     # Preview which cookies would be cleared
actionbook browser cookies clear -y            # Skip confirmation prompt
```

## Configuration

### config - Manage settings

```bash
actionbook config show                         # Display full configuration
actionbook config get <key>                    # Get specific config value
actionbook config set <key> <value>            # Set config value
actionbook config edit                         # Open config in $EDITOR
actionbook config path                         # Show config file location
actionbook config reset                        # Reset configuration (delete config file)
```

**Config keys:**
- `api.base_url` - API endpoint (default: https://api.actionbook.dev)
- `api.api_key` - API authentication key
- `browser.mode` - Browser mode: `isolated` (dedicated debug browser) or `extension` (user's Chrome via bridge)
- `browser.executable` - Browser path override
- `browser.default_profile` - Default profile name (default: "actionbook")
- `browser.headless` - Headless mode (true/false)
- `browser.extension.port` - Extension bridge WebSocket port (default: 19222)
- `browser.extension.auto_install` - Auto-install extension on first use (default: true)
- `browser.backend` - Browser backend: `cdp` or `camofox`
- `browser.camofox.port` - Camoufox server port (default: 9377)

### profile - Manage browser profiles

```bash
actionbook profile list                        # List all profiles
actionbook profile create <name>               # Create new profile
actionbook profile create <name> --cdp-port 9222  # With specific CDP port
actionbook profile delete <name>               # Delete profile
actionbook profile show <name>                 # Show profile details
```

## Extension Management

Manage the Chrome Extension bridge for controlling the user's existing browser.

```bash
actionbook extension status                    # Check if the bridge server is running
actionbook extension status --port 19222       # Check on specific port
actionbook extension ping                      # Ping the extension through the bridge
actionbook extension install                   # Download and install the Chrome extension
actionbook extension install --force           # Force reinstall even if same version
actionbook extension stop                      # Stop the running bridge server
actionbook extension path                      # Print the extension install directory
actionbook extension uninstall                 # Remove the installed extension
```

## Setup

Initial setup wizard for configuring Actionbook.

```bash
actionbook setup                               # Interactive setup wizard
actionbook setup --target claude               # Set up for specific platform (claude, codex, cursor, windsurf, antigravity, opencode, standalone, all)
actionbook setup --api-key <key>               # Provide API key non-interactively
actionbook setup --browser isolated            # Set browser mode (isolated or extension)
actionbook setup --non-interactive             # Skip interactive prompts
actionbook setup --reset                       # Reset existing configuration and start fresh
```

## Global Flags

These flags can be used with any command:

```bash
actionbook --json <command>                    # Output in JSON format
actionbook --headless <command>                # Run browser in headless mode
actionbook --verbose <command>                 # Enable verbose logging
actionbook -P <profile> <command>              # Use specific browser profile
actionbook --cdp <port|url> <command>          # Connect via CDP port or WebSocket URL
actionbook --browser-path <path> <command>     # Override browser executable path
actionbook --api-key <key> <command>           # API key for authenticated access
actionbook --browser-mode isolated <command>   # Use dedicated debug browser (default)
actionbook --browser-mode extension <command>  # Use Chrome Extension bridge with user's browser
actionbook --stealth <command>                 # Enable stealth mode
actionbook --stealth-os <os> <command>         # Stealth OS profile: windows, macos-intel, macos-arm, linux
actionbook --stealth-gpu <gpu> <command>       # Stealth GPU profile (e.g., nvidia-rtx4080, apple-m4-max)
actionbook --camofox <command>                 # Use Camoufox browser backend
actionbook --camofox-port <port> <command>     # Camoufox server port
```

## Environment Variables

```bash
ACTIONBOOK_API_KEY="your-key"                     # API key for authenticated access
ACTIONBOOK_BROWSER_PATH="/path/to/chrome"         # Browser executable
ACTIONBOOK_BROWSER_MODE="isolated"                # Browser mode: isolated or extension
ACTIONBOOK_CDP="9222"                             # CDP port or WebSocket URL
ACTIONBOOK_PROFILE="default"                      # Default profile name
ACTIONBOOK_HEADLESS="true"                        # Headless mode
ACTIONBOOK_STEALTH="true"                         # Enable stealth mode
ACTIONBOOK_STEALTH_OS="macos-arm"                 # Stealth OS profile
ACTIONBOOK_STEALTH_GPU="apple-m4-max"             # Stealth GPU profile
ACTIONBOOK_CAMOFOX="true"                         # Use Camoufox backend
ACTIONBOOK_CAMOFOX_PORT="9377"                    # Camoufox server port
```

> **Note:** `ACTIONBOOK_API_URL` is recognized by the MCP Server and JS SDK (`@actionbookdev/mcp`, `@actionbookdev/sdk`) but **not** by the Rust CLI. For the CLI, use `actionbook config set api.base_url <url>` instead.

## Practical Examples

### Form Submission

```bash
actionbook browser open "https://example.com/form"
actionbook browser snapshot
# Read the snapshot to find selectors

actionbook browser fill "#email" "user@example.com"
actionbook browser fill "#password" "password123"
actionbook browser click "button[type=submit]"
actionbook browser wait-nav
actionbook browser text "h1"  # Check result
```

### Multi-page Navigation

```bash
actionbook browser open "https://example.com"
actionbook browser click "a[href='/products']"
actionbook browser wait-nav
actionbook browser click ".product-card:first-child a"
actionbook browser wait-nav
actionbook browser text ".product-details"
actionbook browser screenshot product.png
```

### Data Extraction

```bash
actionbook browser open "https://example.com/data"
actionbook browser wait-nav
actionbook browser text ".results-table"       # Get table text
actionbook browser eval "JSON.stringify([...document.querySelectorAll('.item')].map(e => e.textContent))"
actionbook browser close
```

### Connect to Existing Browser

```bash
# Start Chrome with debugging port
# google-chrome --remote-debugging-port=9222

actionbook browser connect 9222
actionbook browser pages
actionbook browser snapshot
```

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

### Dialog Handling

Handle JavaScript dialogs (alert, confirm, prompt, beforeunload). When a dialog is open, all other browser commands will block until it is resolved.

```bash
actionbook browser dialog status               # Check if a dialog is currently open
actionbook browser dialog accept               # Accept dialog (OK / confirm)
actionbook browser dialog accept "my input"    # Accept prompt dialog with text
actionbook browser dialog dismiss              # Dismiss/cancel dialog
```

The CLI automatically warns on stderr when a pending dialog is detected during any command. If commands start timing out, check `dialog status` first.

### Cookie Management

```bash
actionbook browser cookies                     # List all cookies (default)
actionbook browser cookies list                # List all cookies
actionbook browser cookies get "<name>"        # Get specific cookie by name
actionbook browser cookies set "<name>" "<value>"              # Set cookie
actionbook browser cookies set "<name>" "<value>" --domain ".example.com"  # Set with domain
actionbook browser cookies delete "<name>"     # Delete specific cookie
actionbook browser cookies clear               # Clear all cookies
```

## Configuration

### config - Manage settings

```bash
actionbook config show                         # Display full configuration
actionbook config get <key>                    # Get specific config value
actionbook config set <key> <value>            # Set config value
actionbook config edit                         # Open config in $EDITOR
actionbook config path                         # Show config file location
```

**Config keys:**
- `api.base_url` - API endpoint (default: https://api.actionbook.dev)
- `api.api_key` - API authentication key
- `browser.executable` - Browser path override
- `browser.default_profile` - Default profile name
- `browser.headless` - Headless mode (true/false)

### profile - Manage browser profiles

```bash
actionbook profile list                        # List all profiles
actionbook profile create <name>               # Create new profile
actionbook profile create <name> --cdp-port 9222  # With specific CDP port
actionbook profile delete <name>               # Delete profile
actionbook profile show <name>                 # Show profile details
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
actionbook --no-daemon <command>              # Disable daemon, use direct WS per command
```

## Environment Variables

```bash
ACTIONBOOK_API_URL="https://api.actionbook.dev"  # API endpoint
ACTIONBOOK_API_KEY="your-key"                     # API authentication key
ACTIONBOOK_BROWSER_PATH="/path/to/chrome"         # Browser executable
ACTIONBOOK_CDP="9222"                             # CDP port or WebSocket URL
ACTIONBOOK_PROFILE="default"                      # Default profile name
ACTIONBOOK_HEADLESS="true"                        # Headless mode
ACTIONBOOK_NO_DAEMON="true"                       # Disable daemon mode
```

## Daemon Management

The daemon runs automatically on Unix + CDP mode. These commands manage daemon lifecycle.

```bash
actionbook daemon status                     # Show daemon status for current profile
actionbook daemon status -P myprofile        # Show daemon status for specific profile
actionbook daemon stop                       # Stop daemon for current profile
actionbook daemon stop -P myprofile          # Stop daemon for specific profile
```

**Note:** `daemon serve` is an internal command used by the auto-start mechanism. Do not call it directly.

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

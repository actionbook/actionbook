# actionbook Command Reference

Complete reference for all `actionbook` CLI commands.

Every browser command requires `--session <SID>`. Most also require `--tab <TID>`.
Session-level commands (start, close, restart, status, list-sessions) need only `--session` or nothing.

Selectors accept CSS, XPath, or snapshot refs (`@eN` from `snapshot` output).

## Global Flags

```
--json            Output as JSON envelope
--timeout <ms>    Command timeout in milliseconds
```

## Action Search & Retrieval

```bash
actionbook search "<query>"                       # Search for action manuals by keyword
actionbook search "<query>" --domain site.com     # Filter by domain
actionbook search "<query>" --url <url>           # Filter by specific URL
actionbook search "<query>" -p 2                  # Page 2 (default: 1)
actionbook search "<query>" -s 20                 # 20 results per page (default: 10, max: 100)
actionbook search "<query>" --domain site.com --url <url> -p 1 -s 5  # Combined

actionbook get "<area_id>"                        # Get complete action details by area ID
# area_id format: "site.com:/path:area_name"
# Examples:
#   actionbook get "airbnb.com:/:default"
#   actionbook get "github.com:/login:form"
#   actionbook get "arxiv.org:/search/advanced:default"
```

`search` returns area IDs with descriptions and relevance scores. Use the area_id with `get` to fetch full details including CSS/XPath selectors, element types, and allowed methods.

## Session

```bash
actionbook browser start                                   # Start a browser session
actionbook browser start --set-session-id s1               # Start with a custom session ID
actionbook browser start --headless                        # Start headless
actionbook browser start --mode cloud --cdp-endpoint <ws>  # Connect to cloud browser
actionbook browser start --open-url https://example.com    # Open URL on start
actionbook browser start --profile myprofile               # Use named profile
actionbook browser start --executable-path /path/to/chrome # Custom browser binary

actionbook browser list-sessions                           # List all active sessions
actionbook browser status --session s1                     # Show session status
actionbook browser close --session s1                      # Close a session
actionbook browser restart --session s1                    # Restart a session
```

## Tab

```bash
actionbook browser list-tabs --session s1                  # List tabs in a session
actionbook browser new-tab https://example.com --session s1  # Open a new tab
actionbook browser new-tab https://example.com --session s1 --new-window  # In new window
actionbook browser close-tab --session s1 --tab t1         # Close a tab
```

`new-tab` is also available as `open`.

## Navigation

```bash
actionbook browser goto <url> --session s1 --tab t1        # Navigate to URL
actionbook browser back --session s1 --tab t1              # Go back
actionbook browser forward --session s1 --tab t1           # Go forward
actionbook browser reload --session s1 --tab t1            # Reload page
```

## Interaction

All interaction commands accept CSS selectors, XPath, or snapshot refs (`@eN`).

```bash
# Click
actionbook browser click "<selector>" --session s1 --tab t1
actionbook browser click 420,310 --session s1 --tab t1        # Click coordinates
actionbook browser click "@e5" --session s1 --tab t1          # Click by snapshot ref
actionbook browser click "<selector>" --count 2 --session s1 --tab t1  # Double-click
actionbook browser click "<selector>" --button right --session s1 --tab t1  # Right-click
actionbook browser click "<selector>" --new-tab --session s1 --tab t1  # Open in new tab

# Text input
actionbook browser fill "<selector>" "text" --session s1 --tab t1   # Clear field, then set value
actionbook browser type "<selector>" "text" --session s1 --tab t1   # Type keystroke by keystroke (appends)

# Keyboard
actionbook browser press Enter --session s1 --tab t1
actionbook browser press Tab --session s1 --tab t1
actionbook browser press Control+A --session s1 --tab t1
actionbook browser press Shift+Tab --session s1 --tab t1

# Selection
actionbook browser select "<selector>" "value" --session s1 --tab t1
actionbook browser select "<selector>" "Display Text" --by-text --session s1 --tab t1

# Mouse
actionbook browser hover "<selector>" --session s1 --tab t1
actionbook browser focus "<selector>" --session s1 --tab t1
actionbook browser mouse-move 420,310 --session s1 --tab t1
actionbook browser cursor-position --session s1 --tab t1
actionbook browser drag "<source>" "<destination>" --session s1 --tab t1

# Scroll
actionbook browser scroll down --session s1 --tab t1
actionbook browser scroll down 500 --session s1 --tab t1            # Scroll down 500px
actionbook browser scroll up --container "#sidebar" --session s1 --tab t1
actionbook browser scroll into-view "@e8" --session s1 --tab t1     # Scroll element into view
actionbook browser scroll into-view "@e8" --align center --session s1 --tab t1
actionbook browser scroll top --session s1 --tab t1                 # Scroll to top
actionbook browser scroll bottom --session s1 --tab t1              # Scroll to bottom

# File upload
actionbook browser upload "<selector>" /path/to/file.pdf --session s1 --tab t1

# JavaScript
actionbook browser eval "document.title" --session s1 --tab t1
actionbook browser eval "document.querySelectorAll('a').length" --session s1 --tab t1
```

**fill vs type:** `fill` clears the field and sets the value directly (like pasting). `type` simulates individual keystrokes and appends to existing content.

## Observation

```bash
# Page info
actionbook browser title --session s1 --tab t1              # Get page title
actionbook browser url --session s1 --tab t1                # Get current URL
actionbook browser viewport --session s1 --tab t1           # Get viewport dimensions

# Content
actionbook browser text --session s1 --tab t1               # Full page text
actionbook browser text "<selector>" --session s1 --tab t1  # Element text
actionbook browser html --session s1 --tab t1               # Full page HTML
actionbook browser html "<selector>" --session s1 --tab t1  # Element outer HTML
actionbook browser value "<selector>" --session s1 --tab t1 # Input element value

# Element inspection
actionbook browser attr "<selector>" href --session s1 --tab t1       # Single attribute
actionbook browser attrs "<selector>" --session s1 --tab t1           # All attributes
actionbook browser box "<selector>" --session s1 --tab t1             # Bounding rect (x, y, width, height)
actionbook browser styles "<selector>" color fontSize --session s1 --tab t1  # Computed styles
actionbook browser describe "<selector>" --session s1 --tab t1        # Full element description
actionbook browser state "<selector>" --session s1 --tab t1           # State flags (visible, enabled, checked, etc.)
actionbook browser inspect-point 420,310 --session s1 --tab t1        # Inspect element at coordinates

# Snapshot
actionbook browser snapshot --session s1 --tab t1                     # Full accessibility tree
actionbook browser snapshot -i --session s1 --tab t1                  # Interactive elements only
actionbook browser snapshot -i -c --session s1 --tab t1               # Interactive + compact
actionbook browser snapshot --depth 3 --session s1 --tab t1           # Limit tree depth
actionbook browser snapshot --selector "#main" --session s1 --tab t1  # Subtree only
```

Output includes a `path` field pointing to the saved snapshot file. Sample output:

```
- generic
  - link "Home" [ref=e8] url=https://example.com/
  - generic
    - combobox "Search" [ref=e9]
    - image "clear" [ref=e10] clickable [cursor:pointer]
  - generic
    - link "Help" [ref=e11] url=https://example.com/help
      - image "Help"
```

The default snapshot contains all information including interactive elements, structural nodes, and cursor-interactive elements. Use additional flags as needed.

Snapshot refs (`@eN`) are **stable across snapshots** — if the element stays the same, the ref stays the same. This lets agents chain commands without re-snapshotting after every step.

### Query

Query elements with cardinality constraints.

```bash
actionbook browser query one "<selector>" --session s1 --tab t1    # Exactly one match (fails on 0 or 2+)
actionbook browser query all "<selector>" --session s1 --tab t1    # All matches (up to 500)
actionbook browser query nth 2 "<selector>" --session s1 --tab t1  # 2nd match (1-based)
actionbook browser query count "<selector>" --session s1 --tab t1  # Match count only
```

Extended pseudo-classes: `:contains("text")`, `:has(child)`, `:visible`, `:enabled`, `:disabled`, `:checked`.

### Screenshots & Export

```bash
actionbook browser screenshot output.png --session s1 --tab t1
actionbook browser screenshot output.png --full --session s1 --tab t1          # Full page
actionbook browser screenshot output.png --annotate --session s1 --tab t1      # Numbered labels
actionbook browser screenshot output.jpg --screenshot-quality 80 --session s1 --tab t1
actionbook browser pdf output.pdf --session s1 --tab t1
```

## Logs

```bash
actionbook browser logs console --session s1 --tab t1                 # All console logs
actionbook browser logs console --level warn,error --session s1 --tab t1  # Filter by level
actionbook browser logs console --tail 10 --session s1 --tab t1      # Last 10 entries
actionbook browser logs console --since log-5 --session s1 --tab t1  # Entries after log-5
actionbook browser logs console --clear --session s1 --tab t1        # Clear after retrieval

actionbook browser logs errors --session s1 --tab t1                 # Uncaught errors + rejections
actionbook browser logs errors --source app.js --session s1 --tab t1 # Filter by source file
actionbook browser logs errors --tail 5 --session s1 --tab t1
actionbook browser logs errors --since err-3 --session s1 --tab t1
actionbook browser logs errors --clear --session s1 --tab t1
```

## Wait

```bash
actionbook browser wait element "<selector>" --session s1 --tab t1              # Wait for element
actionbook browser wait element "<selector>" --timeout 5000 --session s1 --tab t1
actionbook browser wait navigation --session s1 --tab t1                        # Wait for navigation
actionbook browser wait network-idle --session s1 --tab t1                      # Wait for network idle
actionbook browser wait condition "document.readyState === 'complete'" --session s1 --tab t1
```

Default timeout for all wait commands: 30000ms. Override with `--timeout <ms>`.

## Cookies

Cookie commands operate at session level (no `--tab` required).

```bash
actionbook browser cookies list --session s1                          # List all cookies
actionbook browser cookies list --domain .example.com --session s1    # Filter by domain
actionbook browser cookies get session_id --session s1                # Get cookie by name
actionbook browser cookies set token abc123 --session s1              # Set a cookie
actionbook browser cookies set token abc123 --domain .example.com --secure --http-only --session s1
actionbook browser cookies delete token --session s1                  # Delete by name
actionbook browser cookies clear --session s1                         # Clear all cookies
actionbook browser cookies clear --domain .example.com --session s1   # Clear by domain
```

## Storage

Commands are identical for `local-storage` and `session-storage`.

```bash
actionbook browser local-storage list --session s1 --tab t1
actionbook browser local-storage get auth_token --session s1 --tab t1
actionbook browser local-storage set theme dark --session s1 --tab t1
actionbook browser local-storage delete auth_token --session s1 --tab t1
actionbook browser local-storage clear cache_key --session s1 --tab t1

# Same for session-storage:
actionbook browser session-storage list --session s1 --tab t1
actionbook browser session-storage get user_id --session s1 --tab t1
actionbook browser session-storage set lang en --session s1 --tab t1
```

## Setup

```bash
actionbook setup                              # Interactive configuration wizard
actionbook setup --non-interactive --api-key <KEY>  # Non-interactive setup
actionbook setup --reset                      # Reset configuration
```

## Practical Examples

### Form Submission

```bash
actionbook browser start --set-session-id s1
actionbook browser goto "https://example.com/form" --session s1 --tab t1
actionbook browser snapshot --session s1 --tab t1
# Read snapshot refs, then use them:
actionbook browser fill "@e3" "user@example.com" --session s1 --tab t1
actionbook browser fill "@e5" "password123" --session s1 --tab t1
actionbook browser click "@e7" --session s1 --tab t1
actionbook browser wait navigation --session s1 --tab t1
actionbook browser text "h1" --session s1 --tab t1
```

### Multi-page Navigation

```bash
actionbook browser start --set-session-id s1
actionbook browser goto "https://example.com" --session s1 --tab t1
actionbook browser snapshot --session s1 --tab t1
actionbook browser click "@e4" --session s1 --tab t1
actionbook browser wait navigation --session s1 --tab t1
actionbook browser snapshot --session s1 --tab t1
actionbook browser click "@e2" --session s1 --tab t1
actionbook browser wait navigation --session s1 --tab t1
actionbook browser text ".product-details" --session s1 --tab t1
actionbook browser screenshot product.png --session s1 --tab t1
```

### Data Extraction

```bash
actionbook browser start --set-session-id s1
actionbook browser goto "https://example.com/data" --session s1 --tab t1
actionbook browser wait network-idle --session s1 --tab t1
actionbook browser text ".results-table" --session s1 --tab t1
actionbook browser eval "JSON.stringify([...document.querySelectorAll('.item')].map(e => e.textContent))" --session s1 --tab t1
actionbook browser close --session s1
```

### Polling for Changes

```bash
# Check for new console errors periodically
actionbook browser logs errors --session s1 --tab t1
# Note the last ID (e.g., err-3), then later:
actionbook browser logs errors --since err-3 --session s1 --tab t1
```

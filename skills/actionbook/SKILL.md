---
name: actionbook
description: This skill should be used when the user needs to automate multi-step website tasks. Activates for browser automation, web scraping, UI testing, or building AI agents. Provides complete action manuals with step-by-step instructions and verified selectors.
---

## When to Use This Skill

Activate this skill when the user:

- Needs to complete a multi-step task ("Send a LinkedIn message", "Book an Airbnb")
- Asks how to interact with a website ("How do I post a tweet?")
- Builds browser-based AI agents or web scrapers
- Writes E2E tests for external websites
- Navigates to any new page during browser automation
- Wants to control their existing Chrome browser (Extension mode)

## Browser Modes

Actionbook supports two browser control modes:

| Mode | Flag | Use Case |
|------|------|----------|
| **CDP** (default) | (none) | Launches a dedicated browser instance via Chrome DevTools Protocol |
| **Extension** | `--extension` | Controls the user's existing Chrome browser via a Chrome Extension + WebSocket bridge |

**When to use Extension mode:**
- The user wants to operate on their already-open Chrome (with existing logins, cookies, tabs)
- The task requires interacting with pages that need the user's real session state
- The user explicitly mentions their Chrome browser, extension, or existing tabs

**When to use CDP mode (default):**
- Clean browser environment is preferred
- Headless automation or CI/CD
- Profile-based session isolation is needed

All `actionbook browser` commands work identically in both modes. The only difference is adding `--extension` flag (or setting `ACTIONBOOK_EXTENSION=1`).

## How to Use

### Phase 1: Get Action Manual

```bash
# Step 1: Search for action manuals
actionbook search "arxiv search papers"
# Returns: area IDs with descriptions

# Step 2: Get the full manual (use area_id from search results)
actionbook get "arxiv.org:/search/advanced:default"
# Returns: Page structure, UI Elements with CSS/XPath selectors
```

### Phase 2: Execute with Browser (CDP mode — default)

```bash
# Step 3: Open browser
actionbook browser open "https://arxiv.org/search/advanced"

# Step 4: Use CSS selectors from Action Manual directly
actionbook browser fill "#terms-0-term" "Neural Network"
actionbook browser select "#terms-0-field" "title"
actionbook browser click "#date-filter_by-2"
actionbook browser fill "#date-year" "2025"
actionbook browser click "form[action='/search/advanced'] button.is-link"

# Step 5: Wait for results
actionbook browser wait-nav

# Step 6: Extract data
actionbook browser text

# Step 7: Close browser
actionbook browser close
```

### Phase 2 (alt): Execute with Extension mode

Extension mode uses identical browser commands — just add `--extension`. But you **must** follow the full lifecycle below.

```bash
# Step 3: Open URL in user's Chrome
actionbook --extension browser open "https://arxiv.org/search/advanced"

# Step 4-7: Same commands, just add --extension
actionbook --extension browser fill "#terms-0-term" "Neural Network"
actionbook --extension browser select "#terms-0-field" "title"
actionbook --extension browser click "#date-filter_by-2"
actionbook --extension browser fill "#date-year" "2025"
actionbook --extension browser click "form[action='/search/advanced'] button.is-link"
actionbook --extension browser wait-nav
actionbook --extension browser text

# Step 8: Cleanup (CRITICAL — see Extension Mode Lifecycle below)
actionbook --extension browser close    # release debug connection FIRST
actionbook extension stop               # then stop bridge server
```

## Action Manual Format

Action manuals return:
- **Page URL** - Target page address
- **Page Structure** - DOM hierarchy and key sections
- **UI Elements** - CSS/XPath selectors with element metadata

```yaml
  ### button_advanced_search

  - ID: button_advanced_search
  - Description: Advanced search navigation button
  - Type: link
  - Allow Methods: click
  - Selectors:
    - role: getByRole('link', { name: 'Advanced Search' }) (confidence: 0.9)
    - css: button.button.is-small.is-cul-darker (confidence: 0.65)
    - xpath: //button[contains(@class, 'button')] (confidence: 0.55)
```

## Action Search Commands

```bash
actionbook search "<query>"                    # Basic search
actionbook search "<query>" --domain site.com  # Filter by domain
actionbook search "<query>" --url <url>        # Filter by URL
actionbook search "<query>" -p 2 -s 20         # Page 2, 20 results

actionbook get "<area_id>"                     # Full details with selectors
# area_id format: "site.com:/path:area_name"

actionbook sources list                        # List available sources
actionbook sources search "<query>"            # Search sources by keyword
```

## Extension Setup & Management

Commands for managing the Chrome Extension bridge:

```bash
actionbook extension install              # Install extension files to local config dir
actionbook extension path                 # Show extension directory (for Chrome "Load unpacked")
actionbook extension serve                # Start WebSocket bridge (keep running in background)
actionbook extension stop                 # Stop the running bridge server (sends SIGTERM)
actionbook extension status               # Check bridge and extension connection status
actionbook extension ping                 # Ping the extension to verify link is alive
```

**Setup flow (one-time):**
1. `actionbook extension install` — extract extension files and register native messaging host
2. Open `chrome://extensions` → enable Developer mode → Load unpacked → select the path from `actionbook extension path`
3. `actionbook extension serve` — start bridge (keep running)
4. Extension auto-connects via native messaging (no manual token needed in most cases). If auto-pairing fails: copy token from `serve` output → paste in extension popup → Save

**Connection check before automation:**
```bash
actionbook extension status    # should show "running"
actionbook extension ping      # should show "responded"
```

## Browser Commands

> All browser commands below work in both CDP and Extension mode.
> For Extension mode, add `--extension` flag or set `ACTIONBOOK_EXTENSION=1`.

### Navigation

```bash
actionbook browser open <url>                  # Open URL in new tab
actionbook browser goto <url>                  # Navigate current page
actionbook browser back                        # Go back
actionbook browser forward                     # Go forward
actionbook browser reload                      # Reload page
actionbook browser pages                       # List open tabs
actionbook browser switch <page_id>            # Switch tab
actionbook browser close                       # Close browser
actionbook browser restart                     # Restart browser
actionbook browser connect <endpoint>          # Connect to existing browser (CDP port or URL)
```

### Interactions (use CSS selectors from Action Manual)

```bash
actionbook browser click "<selector>"                  # Click element
actionbook browser click "<selector>" --wait 1000      # Wait then click
actionbook browser fill "<selector>" "text"            # Clear and type
actionbook browser type "<selector>" "text"            # Append text
actionbook browser select "<selector>" "value"         # Select dropdown
actionbook browser hover "<selector>"                  # Hover
actionbook browser focus "<selector>"                  # Focus
actionbook browser press Enter                         # Press key
```

### Get Information

```bash
actionbook browser text                        # Full page text
actionbook browser text "<selector>"           # Element text
actionbook browser html                        # Full page HTML
actionbook browser html "<selector>"           # Element HTML
actionbook browser snapshot                    # Accessibility tree
actionbook browser viewport                    # Viewport dimensions
actionbook browser status                      # Browser detection info
```

### Wait

```bash
actionbook browser wait "<selector>"                   # Wait for element
actionbook browser wait "<selector>" --timeout 5000    # Custom timeout
actionbook browser wait-nav                            # Wait for navigation
```

### Screenshots & Export

```bash
# Ensure target directory exists before saving screenshots
actionbook browser screenshot                  # Save screenshot.png
actionbook browser screenshot output.png       # Custom path
actionbook browser screenshot --full-page      # Full page
actionbook browser pdf output.pdf              # Export as PDF
```

### JavaScript & Inspection

```bash
actionbook browser eval "document.title"               # Execute JS
actionbook browser inspect 100 200                     # Inspect at coordinates
actionbook browser inspect 100 200 --desc "login btn"  # With description
```

### Cookies

```bash
actionbook browser cookies list                # List all cookies
actionbook browser cookies get "name"          # Get cookie
actionbook browser cookies set "name" "value"  # Set cookie
actionbook browser cookies set "name" "value" --domain ".example.com"
actionbook browser cookies delete "name"       # Delete cookie
actionbook browser cookies clear               # Clear all
```

## Global Flags

```bash
actionbook --json <command>              # JSON output
actionbook --headless <command>          # Headless mode (CDP only)
actionbook --verbose <command>           # Verbose logging
actionbook -P <profile> <command>        # Use specific profile (CDP only)
actionbook --cdp <port|url> <command>    # CDP connection
actionbook --extension <command>         # Use Chrome Extension mode
# or: ACTIONBOOK_EXTENSION=1 actionbook <command>
```

## Guidelines

- Search by task description, not element name ("arxiv search papers" not "search button")
- **Use Action Manual selectors first** - they are pre-verified and don't require snapshot
- Prefer CSS ID selectors (`#id`) over XPath when both are provided
- **Fallback to snapshot when selectors fail** - use `actionbook browser snapshot` then CSS selectors from the output
- Re-snapshot after navigation - DOM changes invalidate previous state
- **Extension mode**: follow the full lifecycle — pre-flight → connect → execute → cleanup (see [Extension Mode Lifecycle](#extension-mode-lifecycle-critical))
- **Extension mode**: verify extension is installed before starting bridge; prefer auto-pair over manual token
- **Extension mode**: always run `browser close` before stopping the bridge to release the debug connection
- **Extension mode**: the user's real browser is being controlled — avoid destructive actions (clearing all cookies, closing all tabs) without confirmation
- **Extension mode**: L3 operations (some cookie/storage modifications) may require manual approval in the extension popup

## Fallback Strategy

### When Fallback is Needed

Actionbook stores pre-computed page data captured at indexing time. This data may become outdated as websites evolve:

- **Selector execution failure** - The returned CSS/XPath selector does not match any element
- **Element mismatch** - The selector matches an element with unexpected type or behavior
- **Multiple selector failures** - Several selectors from the same action fail consecutively

### Fallback Approaches

When Action Manual selectors don't work:

1. **Snapshot the page** - `actionbook browser snapshot` to get the current accessibility tree
2. **Inspect visually** - `actionbook browser screenshot` to see the current state
3. **Inspect by coordinates** - `actionbook browser inspect <x> <y>` to find elements
4. **Execute JS** - `actionbook browser eval "document.querySelector(...)"` for dynamic queries

### When to Exit

If actionbook search returns no results or action fails unexpectedly, use other available tools to continue the task.

## Examples

### End-to-end with Action Manual

```bash
# 1. Find selectors
actionbook search "airbnb search" --domain airbnb.com

# 2. Get detailed selectors (area_id from search results)
actionbook get "airbnb.com:/:default"

# 3. Automate using pre-verified selectors
actionbook browser open "https://www.airbnb.com"
actionbook browser fill "input[data-testid='structured-search-input-field-query']" "Tokyo"
actionbook browser click "button[data-testid='structured-search-input-search-button']"
actionbook browser wait-nav
actionbook browser text
actionbook browser close
```

### Extension mode: Operate on user's Chrome

```bash
# Verify bridge is running
actionbook extension status

# Use the user's existing logged-in session
actionbook --extension browser open "https://github.com/notifications"
actionbook --extension browser wait-nav
actionbook --extension browser text ".notifications-list"
actionbook --extension browser screenshot notifications.png
```

### Extension Mode Lifecycle (CRITICAL)

When using Extension mode, **always** follow this complete lifecycle: pre-flight → connect → execute → cleanup.

#### 1. Pre-flight: Ask user about extension installation

Before any technical checks, **ask the user** whether they have the Actionbook Chrome Extension installed.

- **User confirms installed** → proceed to Step 2 (Connect).
- **User says not installed** → run the installation flow:

```bash
# Install extension files locally
actionbook extension install
actionbook extension path
# → On macOS, copy to visible dir if needed:
#    cp -r "$(actionbook extension path)" ~/Document/actionbook-extension
```

Then guide the user to load it in Chrome:
1. Open `chrome://extensions` → enable Developer mode
2. Click "Load unpacked" → select the extension directory
3. After user confirms loaded → proceed to Step 2

> **Limitation:** The CLI can only verify that extension files exist locally. There is no way to detect whether Chrome has actually loaded the extension until a connection is attempted in Step 2.

#### 2. Connect: Start bridge, auto-pair with retry

Start the bridge server and attempt auto-pairing. **Retry up to 3 times** before considering manual fallback.

```bash
# Start bridge (run in background)
actionbook extension serve

# Attempt 1: Wait for auto-pairing via Native Messaging
sleep 3
actionbook extension ping
# → If ping succeeds → proceed to Step 3 (Execute)

# Attempt 2: If ping fails, wait longer and retry
sleep 5
actionbook extension ping
# → If ping succeeds → proceed to Step 3 (Execute)

# Attempt 3: Final retry
sleep 5
actionbook extension ping
# → If ping succeeds → proceed to Step 3 (Execute)
```

**Only after all 3 auto-pair attempts fail**, escalate based on the error:

- **"Extension not connected"** → Ask user to verify the extension is enabled in `chrome://extensions` and retry
- **"Invalid token"** → Only now provide the token from `serve` output for manual paste in the extension popup
- **Other errors** → Check `actionbook extension status` for diagnostics

**IMPORTANT:** Do NOT expose the session token prematurely. The token is a last-resort fallback — most users will connect successfully via auto-pair within 3 attempts.

#### 3. Execute: Browser automation

```bash
actionbook --extension browser open "https://example.com"
# ... perform browser operations ...
```

#### 4. Cleanup: Release debug connection, THEN stop bridge

```bash
# Step 1: Release the debugging connection (MUST come first)
actionbook --extension browser close

# Step 2: Stop the bridge server
actionbook extension stop

# Step 3: Verify Chrome no longer shows "debugging" banner
actionbook extension status    # should show "not running"
```

**WARNING:** Skipping Step 1 and directly killing the bridge process will leave Chrome showing "Actionbook is debugging this browser". Always release the debug connection before stopping the bridge.

### Extension mode: Troubleshooting

```bash
# Bridge not running?
actionbook extension serve              # Start it

# Extension not responding?
actionbook extension ping               # Check connectivity

# Token expired? (idle > 30 min)
# Restart serve and re-pair in extension popup
actionbook extension serve              # Prints new token
```

### Deep-Dive Documentation

For detailed patterns and best practices:

| Reference | Description |
|-----------|-------------|
| [references/command-reference.md](references/command-reference.md) | Complete command reference with all features |
| [references/authentication.md](references/authentication.md) | Login flows, OAuth, 2FA handling, state reuse |
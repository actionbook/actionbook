# Actionbook CLI (Rust)

A high-performance CLI for browser automation with zero installation. Built in Rust for speed and reliability.

## Design Principles

| Principle | Description | Implementation |
|-----------|-------------|----------------|
| **Zero Installation** | Use existing system browser, no downloads | Auto-detect Chrome/Brave/Edge/Arc |
| **CDP-First** | Direct Chrome DevTools Protocol control | WebSocket via `chromiumoxide` |
| **Config Flexibility** | Override at any level | CLI > env > config file > auto-discovery |
| **Multi-Profile** | Isolated browser sessions | Profile-based user data dirs |
| **Session Persistence** | Maintain state across commands | Disk-based session storage |
| **Stealth Mode** | Anti-detection browser automation | Fingerprint spoofing, navigator override |
| **API Key Auth** | Authenticated API access | `--api-key` / `ACTIONBOOK_API_KEY` |

### Why These Principles?

```
┌─────────────────────────────────────────────────────────────┐
│                    Traditional Approach                      │
│  ┌─────────┐    ┌─────────┐    ┌─────────┐                 │
│  │ Download │ -> │ Install │ -> │  Run    │   (Slow, Heavy) │
│  │ Chromium │    │ Driver  │    │  Tests  │                 │
│  └─────────┘    └─────────┘    └─────────┘                 │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                    Actionbook Approach                       │
│  ┌─────────────────────────────────────┐                    │
│  │  Use Existing Browser via CDP       │   (Fast, Light)    │
│  │  Chrome/Brave/Edge already installed │                    │
│  └─────────────────────────────────────┘                    │
└─────────────────────────────────────────────────────────────┘
```

## Comparison

| Feature | actionbook-rs | actionbook (TS) | agent-browser |
|---------|---------------|-----------------|---------------|
| **Language** | Rust | TypeScript | TypeScript |
| **Binary Size** | 7.8 MB | ~150 MB (Node.js) | ~200 MB |
| **Startup Time** | ~5ms | ~500ms | ~800ms |
| **CDP Control** | Native (chromiumoxide) | Proxy to agent-browser | Puppeteer |
| **Browser Download** | No | No | Optional |
| **Actionbook API** | Built-in | Built-in | - |
| **Multi-Profile** | Yes | Yes | Yes |
| **Session Persistence** | Disk-based | Disk-based | Memory |
| **Stealth Mode** | Yes (built-in) | - | - |
| **API Key Auth** | Yes | - | - |
| **Headless Mode** | Yes | Yes | Yes |
| **Cookie Management** | Yes | Yes | Yes |
| **PDF Export** | Yes | Yes | Yes |
| **Screenshot** | Yes | Yes | Yes |
| **Scroll** | Yes (5 directions) | Yes | Yes |
| **Batch Execution** | Yes (JSON) | - | - |
| **Fingerprint Rotation** | Yes (dynamic) | - | - |
| **Animation Disabling** | Yes (CSS + media) | - | - |
| **A11y Snapshot** | Yes (token truncation) | - | Yes |
| **Human-Like Input** | Yes (bezier + typing) | - | - |
| **Dependencies** | 0 runtime | Node.js 20+ | Node.js 20+ |

### When to Use Which?

| Use Case | Recommended |
|----------|-------------|
| CI/CD pipelines | **actionbook-rs** (fast startup, no runtime) |
| AI Agent automation | **actionbook (TS)** + MCP |
| Quick scripting | **actionbook-rs** (single binary) |
| Complex browser logic | **agent-browser** (full Puppeteer API) |
| Production deployment | **actionbook-rs** (minimal footprint) |

## Features

- **Zero Installation** - Uses your existing browser (Chrome, Brave, Edge, Arc)
- **CDP-First** - Direct Chrome DevTools Protocol control via WebSocket
- **Actionbook Integration** - Search and retrieve pre-recorded website selectors
- **Multi-Profile** - Isolated browser sessions with persistent state
- **Stealth Mode** - Anti-detection with OS/GPU fingerprint spoofing, navigator override, WebGL emulation
- **API Key Auth** - Authenticated API access via `--api-key` flag or `ACTIONBOOK_API_KEY` env var
- **Accessibility Snapshot** - CDP-based a11y tree with refs, token truncation, and diff mode
- **Flexible Configuration** - CLI args > env vars > config file > auto-discovery
- **Human-Like Input** - Bezier curve mouse movement, realistic typing with typos and hesitation
- **Readability Extraction** - Clean text extraction stripping nav/ads/chrome
- **Resource Blocking** - Block images/media/CSS for faster page loads
- **Animation Disabling** - CSS injection to disable all animations and transitions
- **Batch Execution** - Run sequences of actions from JSON with error control
- **Fingerprint Rotation** - Dynamically change UA, screen, hardware fingerprint
- **Chrome Session Integrity** - Clean stale lock files, prevent "didn't shut down correctly" bar
- **Scroll** - 5 directions (down, up, top, bottom, to element) with smooth option
- **Console Log Capture** - Intercept and display page console.log/warn/error messages
- **Network Idle Wait** - Wait for all network requests to complete (SPA-friendly)
- **Dialog Auto-Handling** - Auto-dismiss JavaScript alert/confirm/prompt dialogs
- **Element Info** - Get element bounding box, attributes, computed styles, and suggested selectors
- **Local Storage Management** - Get, set, remove, clear, and list localStorage/sessionStorage
- **Device Emulation** - Emulate mobile/tablet devices with preset viewports (iPhone, Pixel, iPad)
- **Wait for JS Condition** - Poll a JavaScript expression until it returns truthy
- **Electron App Automation** - Automate desktop apps (VS Code, Slack, Discord, Figma) via `actionbook app` command
- **Keyboard Hotkeys** - Send keyboard combinations (Ctrl+C, Cmd+A, multi-modifier shortcuts)
- **Shadow DOM Support** - Interact with elements inside Shadow DOM using `::shadow-root` selector syntax
- **IFrame Context Switching** - Switch between main frame and iframes for embedded content
- **Scroll with Wait** - Scroll and wait for `scrollend` event, essential for lazy-loaded content

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Actionbook CLI                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌──────────┐   ┌──────────┐   ┌──────────┐   ┌──────────┐    │
│  │  search  │   │   get    │   │ sources  │   │  browser │    │
│  │ command  │   │ command  │   │ command  │   │ commands │    │
│  └────┬─────┘   └────┬─────┘   └────┬─────┘   └────┬─────┘    │
│       │              │              │              │           │
│       └──────────────┴──────────────┴──────────────┘           │
│                          │                                      │
│  ┌───────────────────────┴───────────────────────────────┐     │
│  │                    Core Modules                        │     │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐   │     │
│  │  │  API Client │  │   Browser   │  │   Config    │   │     │
│  │  │  (reqwest)  │  │   (CDP)     │  │  (figment)  │   │     │
│  │  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘   │     │
│  └─────────┼────────────────┼────────────────┼──────────┘     │
│            │                │                │                 │
└────────────┼────────────────┼────────────────┼─────────────────┘
             │                │                │
             ▼                ▼                ▼
    ┌────────────────┐ ┌─────────────┐ ┌──────────────┐
    │  Actionbook    │ │   Chrome    │ │ ~/.config/   │
    │  API Server    │ │   Browser   │ │ actionbook/  │
    └────────────────┘ └─────────────┘ └──────────────┘
```

## Module Structure

```
src/
├── main.rs                  # Entry point, tracing setup
├── cli.rs                   # CLI argument definitions (clap)
├── error.rs                 # Error types (thiserror)
├── api/
│   ├── mod.rs               # API module exports
│   ├── client.rs            # Actionbook API client
│   └── types.rs             # API request/response types
├── browser/
│   ├── mod.rs               # Browser module exports
│   ├── discovery.rs         # Auto-detect installed browsers
│   ├── launcher.rs          # Launch browser with CDP + session integrity (G3)
│   ├── session.rs           # Session state, animations (G2), fingerprint (G5)
│   ├── router.rs            # BrowserDriver multi-backend dispatch
│   ├── snapshot.rs          # CDP Accessibility Tree (F1), diff (F6), truncation (G1)
│   ├── readability.rs       # Readability text extraction (F4)
│   ├── human_input.rs       # Human-like mouse/typing (F5)
│   ├── content.rs           # Content extraction utilities
│   ├── stealth.rs           # Stealth mode (anti-detection profiles)
│   ├── stealth_enhanced.rs  # Enhanced stealth (Camoufox techniques)
│   ├── fingerprint_generator.rs  # Statistical fingerprint generation
│   ├── human_behavior.rs    # Human behavior simulation
│   ├── extension_bridge.rs  # Chrome Extension WebSocket bridge
│   ├── extension_installer.rs    # Extension download/install
│   └── native_messaging.rs  # Chrome Native Messaging host
├── config/
│   ├── mod.rs               # Configuration loading
│   └── profile.rs           # Profile management
└── commands/
    ├── mod.rs               # Command module exports
    ├── search.rs            # Search actions command
    ├── get.rs               # Get action by ID command
    ├── sources.rs           # List/search sources command
    ├── browser.rs           # Browser automation commands
    ├── batch.rs             # Batch action execution (G4)
    ├── act.rs               # Action execution command
    ├── execute.rs           # Execute action on element
    ├── record.rs            # Record browser actions
    ├── replay.rs            # Replay recorded actions
    ├── validate.rs          # Validate selectors
    ├── config.rs            # Config management commands
    ├── profile.rs           # Profile management commands
    └── setup/               # Interactive setup wizard
```

## Prerequisites

| Requirement | Description |
|-------------|-------------|
| **Chromium-based Browser** | One of: Google Chrome, Brave, Microsoft Edge, Arc, or Chromium |

### What You DON'T Need

| Traditional Tools | actionbook-rs |
|-------------------|---------------|
| Node.js runtime | **Not needed** |
| Download Chromium | **Not needed** (uses system browser) |
| WebDriver/Selenium | **Not needed** (direct CDP) |
| Python | **Not needed** |

### Verify Prerequisites

```bash
# Check if a supported browser is detected
actionbook browser status
```

## Installation

### From Source

```bash
git clone https://github.com/actionbook/actionbook.git
cd actionbook/packages/actionbook-rs
cargo build --release

# Binary at: ./target/release/actionbook
```

### Binary Size

The release binary is ~7.8 MB (with LTO and strip enabled).

## Quick Start

### 1. Search for Website Actions

```bash
# Search for Etsy-related actions
actionbook search "etsy"

# Search with domain filter
actionbook search "valentine" --domain etsy.com

# Paginated results
actionbook search "login" --page 1 --page-size 5
```

### 2. Get Action Details

```bash
# Get full action details by area ID
# Area ID format: site:path:area (e.g., "airbnb.com:/:default")
actionbook get "etsy.com:/:search_form"
```

### 3. Browser Automation

```bash
# Check browser status
actionbook browser status

# Open a URL in browser
actionbook browser open "https://www.etsy.com"

# Navigate to a page
actionbook browser goto "https://www.etsy.com/market/valentines_day_gifts"

# Take a screenshot
actionbook browser screenshot /tmp/screenshot.png

# Click an element
actionbook browser click "button.search-btn"

# Type text into an input
actionbook browser type "input[name=search]" "valentine gifts"

# Execute JavaScript
actionbook browser eval "document.title"
```

### 4. Configuration

```bash
# Show current configuration
actionbook config show

# Get config file path
actionbook config path

# Set a config value
actionbook config set api_url "https://api.actionbook.dev"
```

### 5. Profile Management

```bash
# List profiles
actionbook profile list

# Create a new profile
actionbook profile create work

# Use a profile
actionbook --profile work browser open "https://example.com"
```

## Configuration

### Config File Location

- macOS: `~/.actionbook/config.toml`
- Linux: `~/.actionbook/config.toml`
- Windows: `%USERPROFILE%\.actionbook\config.toml`

### Example Config

```toml
[api]
base_url = "https://api.actionbook.dev"
api_key = "sk-your-api-key"    # Optional, for authenticated access

[browser]
headless = false
default_profile = "actionbook"

[profiles.actionbook]
cdp_port = 9222
headless = false

[profiles.headless]
cdp_port = 9223
headless = true
```

By default, each profile uses an isolated browser data directory:
`<data_dir>/actionbook/profiles/<profile>`.

### Environment Variables

All config values can be overridden via environment variables:

```bash
# API
ACTIONBOOK_API_KEY=sk-your-api-key

# Browser
ACTIONBOOK_HEADLESS=true
ACTIONBOOK_BROWSER_PATH=/usr/bin/google-chrome

# Stealth
ACTIONBOOK_STEALTH=true
ACTIONBOOK_STEALTH_OS=macos-arm
ACTIONBOOK_STEALTH_GPU=apple-m4-max
```

### Configuration Priority

```
CLI args > Environment variables > Config file > Auto-discovery
```

## Global Flags

| Flag | Env Var | Description |
|------|---------|-------------|
| `--json` | | Output in JSON format |
| `--verbose` | | Enable verbose logging |
| `--headless` | `ACTIONBOOK_HEADLESS` | Run browser in headless mode |
| `--profile <NAME>` | `ACTIONBOOK_PROFILE` | Use specific profile |
| `--browser-path <PATH>` | `ACTIONBOOK_BROWSER_PATH` | Custom browser executable path |
| `--cdp <PORT>` | `ACTIONBOOK_CDP` | Connect to existing CDP port |
| `--api-key <KEY>` | `ACTIONBOOK_API_KEY` | API key for authenticated access |
| `--stealth` | `ACTIONBOOK_STEALTH` | Enable stealth mode (anti-detection) |
| `--stealth-os <OS>` | `ACTIONBOOK_STEALTH_OS` | Stealth OS: windows, macos-arm, macos-intel, linux |
| `--stealth-gpu <GPU>` | `ACTIONBOOK_STEALTH_GPU` | Stealth GPU: rtx4080, apple-m4-max, intel-uhd630, etc. |
| `--block-images` | `ACTIONBOOK_BLOCK_IMAGES` | Block image downloads for faster page loads |
| `--block-media` | `ACTIONBOOK_BLOCK_MEDIA` | Block images, fonts, CSS, and media |
| `--no-animations` | `ACTIONBOOK_NO_ANIMATIONS` | Disable CSS animations, transitions, and smooth scrolling |
| `--auto-dismiss-dialogs` | `ACTIONBOOK_AUTO_DISMISS_DIALOGS` | Auto-dismiss JS alert/confirm/prompt dialogs |

## Commands Reference

### `search` - Search Actions

```bash
actionbook search <QUERY> [OPTIONS]

Options:
  -d, --domain <DOMAIN>     Filter by domain (e.g., "airbnb.com")
  -u, --url <URL>           Filter by specific URL
  -p, --page <N>            Page number [default: 1]
  -s, --page-size <N>       Results per page (1-100) [default: 10]
```

Output: Plain text with area_id list for next step.

### `get` - Get Action by Area ID

```bash
actionbook get <AREA_ID>

# Area ID format: site:path:area
# Examples:
actionbook get "airbnb.com:/:default"
actionbook get "etsy.com:/search:search_results"
```

Output: Plain text with element selectors and methods.

### `sources` - Manage Sources

```bash
actionbook sources list [--limit <N>]
actionbook sources search <QUERY> [--limit <N>]
```

### `browser` - Browser Automation

```bash
actionbook browser status           # Show connection status
actionbook browser open <URL>       # Open URL in new browser
actionbook browser goto <URL>       # Navigate current page
actionbook browser click <SELECTOR> # Click element
actionbook browser type <SELECTOR> <TEXT>  # Type text
actionbook browser fill <SELECTOR> <TEXT>  # Fill input field
actionbook browser wait <SELECTOR>  # Wait for element
actionbook browser screenshot [PATH]       # Take screenshot
actionbook browser pdf <PATH>       # Save as PDF
actionbook browser eval <CODE>      # Execute JavaScript
actionbook browser snapshot          # Accessibility tree snapshot
actionbook browser snapshot --max-tokens 500  # Truncated snapshot for LLM context
actionbook browser inspect <X> <Y>  # Inspect element at coordinates
actionbook browser viewport         # Show viewport size
actionbook browser scroll down [PIXELS]     # Scroll down (default: viewport height)
actionbook browser scroll down [PIXELS] --wait  # Scroll and wait for scrollend event
actionbook browser scroll up [PIXELS]       # Scroll up
actionbook browser scroll bottom            # Scroll to page bottom
actionbook browser scroll top               # Scroll to page top
actionbook browser scroll to <SELECTOR>     # Scroll to element
actionbook browser hotkey <KEYS>            # Send keyboard combination (e.g., "Control+C", "Meta+A")
actionbook browser switch-frame <TARGET>    # Switch to iframe ("default"/"parent" or selector)
actionbook browser batch --file actions.json  # Execute batch of actions
actionbook browser fingerprint rotate       # Rotate browser fingerprint
actionbook browser console                  # Capture console log messages
actionbook browser console --duration 5000  # Listen for 5 seconds
actionbook browser console --level error    # Errors only
actionbook browser wait-idle                # Wait for network idle
actionbook browser wait-idle --timeout 10000 --idle-time 1000
actionbook browser info <SELECTOR>          # Get element info (bbox, attrs, styles)
actionbook browser storage list             # List all localStorage keys
actionbook browser storage get <KEY>        # Get localStorage value
actionbook browser storage set <KEY> <VALUE>  # Set localStorage value
actionbook browser storage clear            # Clear localStorage
actionbook browser storage list --session   # Use sessionStorage
actionbook browser emulate iphone-14        # Emulate iPhone 14
actionbook browser emulate pixel-7          # Emulate Pixel 7
actionbook browser emulate ipad             # Emulate iPad
actionbook browser emulate 1280x720         # Custom resolution
actionbook browser wait-fn "document.querySelector('#done')"  # Wait for condition
actionbook browser wait-fn "window.loaded === true" --timeout 10000
actionbook browser connect <PORT>       # Connect via local CDP port
actionbook browser connect <WSS_URL>    # Connect via remote WebSocket URL
actionbook browser close            # Close browser
actionbook browser restart          # Restart browser
actionbook browser cookies list     # List cookies
actionbook browser cookies get <NAME>      # Get cookie
actionbook browser cookies set <NAME> <VALUE>  # Set cookie
actionbook browser cookies delete <NAME>   # Delete cookie
actionbook browser cookies clear    # Clear all cookies
```

`actionbook browser` no longer auto-attaches to local CDP ports (9222/9223/9224).
Use `actionbook browser connect <PORT|WS_URL>` explicitly when you want to reuse an existing browser.

For remote WebSocket endpoints that require authentication headers (e.g. SigV4-signed URLs), optionally pass `-H`/`--header` flags:

```bash
actionbook browser connect "wss://example.com/automation" \
  -H "authorization:AWS4-HMAC-SHA256 ..." \
  -H "x-amz-date:20260312T080000Z"
```

Headers are persisted in the session state and used for all subsequent WebSocket connections (page enumeration, CDP commands, liveness probes).

### `config` - Configuration

```bash
actionbook config show              # Show all config
actionbook config path              # Show config file path
actionbook config get <KEY>         # Get config value
actionbook config set <KEY> <VALUE> # Set config value
```

### `profile` - Profile Management

```bash
actionbook profile list             # List all profiles
actionbook profile create <NAME>    # Create new profile
actionbook profile delete <NAME>    # Delete profile
```

### `app` - Electron App Automation

Automate desktop applications built with Electron (VS Code, Slack, Discord, Figma, Notion, Spotify, etc.) using Chrome DevTools Protocol.

**App-Specific Commands:**

```bash
actionbook app launch <NAME>        # Auto-discover and launch Electron app
actionbook app attach <PORT>        # Attach to running app on CDP port
actionbook app list                 # List all discoverable Electron apps
actionbook app status               # Show current app connection status
actionbook app close                # Close the connected app
actionbook app restart              # Restart the app (preserves session)
```

**All Browser Commands Available:**

Every `browser` command works with `app` prefix (35+ commands):

```bash
# Navigation
actionbook app goto <URL>

# Interaction
actionbook app click <SELECTOR>
actionbook app type <TEXT> [SELECTOR]
actionbook app fill <TEXT> [SELECTOR]
actionbook app hotkey "Control+C"

# Analysis
actionbook app snapshot [--format compact]
actionbook app screenshot output.png
actionbook app eval "document.title"

# Tab management
actionbook app tab list
actionbook app tab new
actionbook app tab switch <PAGE_ID>
```

**Example Workflows:**

```bash
# Automate VS Code
actionbook app launch "Visual Studio Code"
actionbook app hotkey "Cmd+Shift+P"      # Open command palette
actionbook app type "Git: Commit"
actionbook app hotkey "Enter"

# Automate Slack
actionbook app launch Slack
actionbook app click "#channel-name"
actionbook app type "Hello team!" "div[role='textbox']"
actionbook app hotkey "Enter"

# Automate Figma
actionbook app attach 9222               # If Figma already running with --remote-debugging-port=9222
actionbook app click "button:has-text('Export')"
actionbook app screenshot design.png
```

**Supported Apps:**

| App | macOS | Linux | Windows |
|-----|-------|-------|---------|
| Visual Studio Code | ✅ | ✅ | ✅ |
| Slack | ✅ | ✅ | ✅ |
| Discord | ✅ | ✅ | ✅ |
| Figma | ✅ | ✅ | ✅ |
| Notion | ✅ | ✅ | ✅ |
| Spotify | ✅ | ✅ | ✅ |
| Any Electron App | ✅ | ✅ | ✅ |

**How It Works:**

1. **Auto-Discovery**: Scans common install locations for Electron apps
2. **CDP Launch**: Starts app with `--remote-debugging-port=9222`
3. **Session Tracking**: Stores app path for restart preservation
4. **Full Feature Parity**: All browser automation features work (Shadow DOM, iframes, hotkeys, etc.)

**Port Inference:**

```bash
# If app already running with CDP enabled
actionbook app attach 9222              # Validates CDP endpoint and matches against known apps
```

## Stealth Mode

Stealth mode applies anti-detection measures to avoid bot detection:

```bash
# Enable stealth with default profile (macOS ARM + Apple M4 Max)
actionbook --stealth browser open "https://example.com"

# Custom OS/GPU fingerprint
actionbook --stealth --stealth-os windows --stealth-gpu rtx4080 browser open "https://example.com"

# Via environment variables
export ACTIONBOOK_STEALTH=true
export ACTIONBOOK_STEALTH_OS=macos-arm
export ACTIONBOOK_STEALTH_GPU=apple-m4-max
actionbook browser open "https://example.com"
```

### What Stealth Mode Does

| Feature | Description |
|---------|-------------|
| **Navigator Override** | Spoofs `navigator.webdriver`, `platform`, `hardwareConcurrency`, `deviceMemory` |
| **WebGL Emulation** | Overrides WebGL renderer/vendor to match selected GPU |
| **Plugin Spoofing** | Injects fake Chrome plugins (PDF, Native Client) |
| **Webdriver Masking** | `navigator.webdriver` override via CDP script injection (no unsupported Chrome flags) |
| **Persistent Injection** | Uses `Page.addScriptToEvaluateOnNewDocument` for cross-navigation persistence |

### Available Profiles

**OS**: `windows`, `macos-intel`, `macos-arm`, `linux`

**GPU**: `rtx4080`, `rtx3080`, `gtx1660`, `rx6800`, `uhd630`, `iris-xe`, `m1-pro`, `m2-max`, `m4-max`

## Scroll

Scroll the page in any direction, with optional smooth animation and wait for completion:

```bash
actionbook browser scroll down              # Down one viewport height
actionbook browser scroll down 500          # Down 500 pixels
actionbook browser scroll down 500 --wait   # Scroll and wait for scrollend event
actionbook browser scroll up 300 --smooth   # Up 300px with smooth animation
actionbook browser scroll up 500 --smooth --wait  # Smooth scroll + wait
actionbook browser scroll bottom            # Scroll to page bottom
actionbook browser scroll top               # Scroll to page top
actionbook browser scroll to "#footer"      # Scroll to element
actionbook browser scroll to ".form" --align start  # Align to top of viewport
```

| Direction | Argument | Description |
|-----------|----------|-------------|
| `down` | `[PIXELS]` | Scroll down (default: viewport height) |
| `up` | `[PIXELS]` | Scroll up (default: viewport height) |
| `bottom` | — | Scroll to absolute bottom |
| `top` | — | Scroll to absolute top |
| `to` | `<SELECTOR>` | Scroll to CSS selector (`--align start\|center\|end\|nearest`) |

### Scroll with Wait (`--wait` flag)

The `--wait` flag makes the scroll command wait for the browser's `scrollend` event, ensuring:
- Lazy-loaded images/content appear after scroll
- DOM is stable before the next action
- No "element not found" errors on dynamic content

```bash
# Essential for infinite scroll and lazy-loaded content
actionbook browser scroll down 1000 --wait
actionbook browser scroll bottom --wait

# Smooth scroll + wait = complete animation before continuing
actionbook browser scroll down 800 --smooth --wait
```

## Accessibility Snapshot

CDP-based accessibility tree extraction for AI agents:

```bash
actionbook browser snapshot                          # Full tree, compact format
actionbook browser snapshot --format text            # Indented text format
actionbook browser snapshot --format json            # JSON format
actionbook browser snapshot --filter interactive     # Only buttons/links/inputs
actionbook browser snapshot --depth 3                # Limit tree depth
actionbook browser snapshot --selector "#main"       # Scope to element
actionbook browser snapshot --diff                   # Show changes since last snapshot
actionbook browser snapshot --max-tokens 500         # Truncate for LLM context
```

### Snapshot Formats

**Compact** (default, ~60-70% fewer tokens):
```
e0:navigation "Main Menu"
e1:link "Home"
e2:searchbox "Search" [focused]
e3:button "Submit"
```

**Text** (indented tree):
```
e0 navigation "Main Menu"
  e1 link "Home"
  e2 searchbox "Search" [focused]
  e3 button "Submit"
```

### Interactive Filter (`--filter interactive`)

Filters the accessibility tree to only show actionable elements — the nodes an AI agent can actually interact with. This dramatically reduces noise from decorative/structural nodes.

**16 supported ARIA roles:**

| Category | Roles |
|----------|-------|
| Form inputs | `textbox`, `searchbox`, `combobox`, `spinbutton`, `slider` |
| Buttons | `button` |
| Links | `link` |
| Selection | `checkbox`, `radio`, `switch`, `menuitem`, `option` |
| Containers | `tab`, `treeitem` |
| Media | `video`, `audio` |

```bash
# Interactive-only snapshot
actionbook browser snapshot --filter interactive

# Combine with token budget (recommended for LLM agents)
actionbook browser snapshot --filter interactive --max-tokens 500

# Interactive + JSON output
actionbook browser snapshot --filter interactive --format json
```

**Typical reduction:** A full page may have 200+ a11y nodes; `--filter interactive` typically returns 20-40 actionable elements. Combined with `--max-tokens`, this gives agents a focused, budget-friendly view of what they can do on the page.

### Token Truncation (`--max-tokens`)

When `--max-tokens N` is set, nodes are included until the token budget is exceeded. A `(truncated to ~N tokens)` notice is appended. In JSON mode, truncated output includes `"truncated": true`.

## Batch Execution

Execute a sequence of browser actions from JSON:

```bash
# From file
actionbook browser batch --file actions.json

# From stdin
echo '{"actions":[{"kind":"goto","url":"https://example.com"},{"kind":"snapshot"}],"stopOnError":true}' | actionbook browser batch

# Custom delay between steps
actionbook browser batch --file actions.json --delay 100
```

### JSON Format

```json
{
  "actions": [
    {"kind": "goto", "url": "https://example.com"},
    {"kind": "click", "selector": "#login"},
    {"kind": "type", "selector": "#email", "text": "user@test.com"},
    {"kind": "click", "selector": "#submit"},
    {"kind": "wait", "selector": ".dashboard", "timeout": 5000},
    {"kind": "snapshot"}
  ],
  "stopOnError": true
}
```

Supported kinds: `goto`, `click`, `type`, `fill`, `select`, `snapshot`, `text`, `screenshot`, `scroll`, `wait`.

### Output

```json
{
  "results": [
    {"index": 0, "kind": "goto", "success": true},
    {"index": 1, "kind": "click", "success": true},
    {"index": 2, "kind": "type", "success": false, "error": "Element not found: #email"}
  ],
  "total": 3,
  "successful": 2,
  "failed": 1
}
```

## Fingerprint Rotation

Dynamically change browser fingerprint on a running session:

```bash
# Random fingerprint
actionbook browser fingerprint rotate

# Specific OS
actionbook browser fingerprint rotate --os windows
actionbook browser fingerprint rotate --os mac
actionbook browser fingerprint rotate --os linux

# Custom screen resolution
actionbook browser fingerprint rotate --os windows --screen 1920x1080
```

Rotates: User-Agent, platform, screen dimensions, hardware concurrency, device memory.

## Animation Disabling

Disable all CSS animations and transitions for stable snapshots and screenshots:

```bash
# Via global flag
actionbook --no-animations browser open "https://animate.style"

# Via environment variable
ACTIONBOOK_NO_ANIMATIONS=true actionbook browser goto "https://example.com"
```

Injects CSS `animation: none !important; transition: none !important;` and sets `prefers-reduced-motion: reduce` via CDP Emulation.

### Console Log Capture

```bash
# Snapshot current console messages
actionbook browser console

# Listen for 5 seconds, errors only
actionbook browser console --duration 5000 --level error

# All levels: all, log, info, warn, error, debug
actionbook browser console --level warn
```

Installs a JS interceptor that captures `console.log/warn/error/info/debug` messages. Up to 200 messages are buffered. The interceptor persists across navigations via `Page.addScriptToEvaluateOnNewDocument`.

### Network Idle Wait

```bash
# Wait until no pending requests for 500ms (default)
actionbook browser wait-idle

# Custom timeout and idle threshold
actionbook browser wait-idle --timeout 10000 --idle-time 1000
```

Monitors `fetch()` and `XMLHttpRequest` to track pending network requests. Returns when no requests have been in-flight for the specified idle time. Essential for SPAs that load data asynchronously after initial page load.

### Dialog Auto-Handling

```bash
# Enable globally — auto-dismiss all JS dialogs
actionbook --auto-dismiss-dialogs browser open "https://example.com"

# Or via environment variable
export ACTIONBOOK_AUTO_DISMISS_DIALOGS=true
```

Overrides `window.alert`, `window.confirm` (returns `true`), and `window.prompt` (returns default value). Logged dismissed dialogs are accessible via the console capture feature. Prevents JavaScript dialogs from blocking agent execution.

### Element Info

```bash
# Get detailed info about an element
actionbook browser info "#search-button"

# Output includes:
#   Element: <button>
#   id: search-button
#   text: "Search"
#   bbox: (120, 340) 200x40
#   visible: yes | interactive: yes
#   selectors:
#     #search-button
#     button.btn.btn-primary
```

Returns element's tag name, bounding box, attributes, computed styles (display, visibility, color, cursor, etc.), visibility status, interactivity detection, and suggested CSS selectors.

### Local Storage Management

```bash
# localStorage operations
actionbook browser storage list              # List all keys
actionbook browser storage get "token"       # Get a value
actionbook browser storage set "key" "val"   # Set a value
actionbook browser storage remove "key"      # Remove a key
actionbook browser storage clear             # Clear all

# sessionStorage — add --session flag
actionbook browser storage list --session
actionbook browser storage get "tab_id" --session
```

### Device Emulation

```bash
# Preset devices
actionbook browser emulate iphone-14       # 390x844 @3x, mobile UA
actionbook browser emulate iphone-se       # 375x667 @2x, mobile UA
actionbook browser emulate pixel-7         # 412x915 @2.625x, mobile UA
actionbook browser emulate ipad            # 820x1180 @2x, mobile UA
actionbook browser emulate desktop-hd      # 1920x1080 @1x
actionbook browser emulate desktop-4k      # 3840x2160 @2x

# Custom resolution (WxH)
actionbook browser emulate 1280x720
actionbook browser emulate 375x812
```

Sets viewport dimensions, device scale factor, user agent, and touch emulation via CDP `Emulation.setDeviceMetricsOverride`.

### Wait for JS Condition

```bash
# Wait for element to appear
actionbook browser wait-fn "document.querySelector('#loaded')"

# Wait for custom flag
actionbook browser wait-fn "window.appReady === true"

# With custom timeout and polling interval
actionbook browser wait-fn "document.title.includes('Done')" --timeout 10000 --interval 200
```

Polls the JavaScript expression at the specified interval until it returns a truthy value (non-null, non-false, non-empty, non-zero). Returns the expression's value on success or times out with an error.

## Keyboard Hotkeys

Send keyboard combinations for shortcuts, copy/paste, and other key-based interactions:

```bash
# Single modifier + key
actionbook browser hotkey "Control+C"        # Copy (Ctrl+C)
actionbook browser hotkey "Meta+V"           # Paste on macOS (Cmd+V)
actionbook browser hotkey "Control+A"        # Select all

# Multiple modifiers
actionbook browser hotkey "Control+Shift+P"  # Command palette (many editors)
actionbook browser hotkey "Meta+Shift+N"     # New private window

# Navigation
actionbook browser hotkey "Control+Tab"      # Next tab
actionbook browser hotkey "Control+W"        # Close tab
actionbook browser hotkey "Meta+R"           # Refresh (macOS)
```

**Modifier Keys:**
- `Control`: Ctrl key (all platforms)
- `Meta`: Cmd key (macOS) / Windows key (Windows)
- `Shift`: Shift key
- `Alt`: Alt/Option key

**Common Patterns:**
```bash
# Copy-paste workflow
actionbook browser hotkey "Control+A"        # Select all
actionbook browser hotkey "Control+C"        # Copy
actionbook browser click "textarea#destination"
actionbook browser hotkey "Control+V"        # Paste

# VS Code command palette
actionbook browser hotkey "Control+Shift+P"
actionbook browser type "input" "Format Document"

# Browser shortcuts
actionbook browser hotkey "Control+T"        # New tab
actionbook browser hotkey "Control+L"        # Focus address bar
actionbook browser hotkey "Control+Plus"     # Zoom in
```

## Shadow DOM Support

Interact with elements inside Shadow DOM using the `::shadow-root` selector syntax. Essential for modern web components (Custom Elements) and frameworks like Lit, Polymer, Stencil.

```bash
# Basic shadow DOM selector
actionbook browser click "custom-button::shadow-root > button"

# Type into shadow input
actionbook browser type "my-input::shadow-root > input" "Hello world"

# Nested shadow DOM
actionbook browser click "outer::shadow-root > inner::shadow-root > button"

# Combined with other selectors
actionbook browser click "app-root::shadow-root > nav > button.primary"
```

**When to Use:**
- Web components with encapsulated styles
- Modern UI libraries (Lit, Stencil, Shoelace)
- Enterprise apps with isolated components

**Example - Shoelace UI components:**
```bash
actionbook browser open "https://shoelace.style"

# Click button inside shadow DOM
actionbook browser click "sl-button::shadow-root > button"

# Type in shadow input
actionbook browser type "sl-input::shadow-root > input" "search term"

# Toggle shadow checkbox
actionbook browser click "sl-checkbox::shadow-root > input"
```

## IFrame Context Switching

Switch between main frame and iframes when pages embed external content (payment forms, embedded apps, widgets):

```bash
# Switch to iframe
actionbook browser switch-frame "iframe#payment"

# Interact inside iframe (all commands work)
actionbook browser type "input#card-number" "4111111111111111"
actionbook browser click "button.submit"

# Switch back to parent frame
actionbook browser switch-frame "parent"

# Switch to main frame (top level)
actionbook browser switch-frame "default"
```

**Frame Targets:**
- `<selector>`: CSS selector for iframe element (e.g., `iframe#checkout`, `iframe.widget`)
- `parent`: Switch to parent frame (one level up)
- `default`: Switch to main/top frame (reset to root)

**Complete Flow - Stripe Payment:**
```bash
# Open checkout page
actionbook browser open "https://example.com/checkout"

# Wait for page load
actionbook browser wait-idle

# Switch to Stripe iframe
actionbook browser switch-frame "iframe[name='stripe_frame']"

# Fill payment details (now inside iframe)
actionbook browser type "input[name='cardnumber']" "4242424242424242"
actionbook browser type "input[name='exp-date']" "12/25"
actionbook browser type "input[name='cvc']" "123"
actionbook browser click "button[type='submit']"

# Switch back to main frame
actionbook browser switch-frame "default"

# Verify confirmation (now in main frame)
actionbook browser wait ".confirmation"
actionbook browser screenshot confirmation.png
```

**Nested IFrames:**
```bash
# Level 0: Main frame
actionbook browser open "https://example.com"

# Level 1: Outer iframe
actionbook browser switch-frame "iframe#outer"

# Level 2: Inner iframe (inside outer)
actionbook browser switch-frame "iframe#inner"

# Interact at level 2
actionbook browser click "button"

# Back to level 1
actionbook browser switch-frame "parent"

# Back to level 0
actionbook browser switch-frame "default"
```

## Supported Browsers

The CLI auto-detects and supports:

| Browser | macOS | Linux | Windows |
|---------|-------|-------|---------|
| Google Chrome | ✓ | ✓ | ✓ |
| Brave | ✓ | ✓ | ✓ |
| Microsoft Edge | ✓ | ✓ | ✓ |
| Arc | ✓ | - | - |
| Chromium | ✓ | ✓ | ✓ |

## Development

### Build

```bash
cargo build           # Debug build
cargo build --release # Release build (optimized)
```

### Test

```bash
cargo test                          # Run all tests
cargo test --test cli_test          # CLI tests only
cargo test --test integration_test  # Integration tests only
```

### Test Coverage

- **109 tests** total (unit + integration)

## License

MIT License - see [LICENSE](LICENSE) for details.

## Related

- [Actionbook MCP](../mcp) - MCP Server for AI agents
- [Actionbook SDK](../js-sdk) - JavaScript SDK
- [Actionbook API](https://api.actionbook.dev) - REST API

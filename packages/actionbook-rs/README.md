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
- **Accessibility Snapshot** - Agent-browser compatible snapshot with refs, text nodes, and `/url:` for links
- **Flexible Configuration** - CLI args > env vars > config file > auto-discovery

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
├── main.rs              # Entry point, tracing setup
├── cli.rs               # CLI argument definitions (clap)
├── error.rs             # Error types (thiserror)
├── api/
│   ├── mod.rs           # API module exports
│   ├── client.rs        # Actionbook API client
│   └── types.rs         # API request/response types
├── browser/
│   ├── mod.rs           # Browser module exports
│   ├── discovery.rs     # Auto-detect installed browsers
│   ├── launcher.rs      # Launch browser with CDP
│   ├── session.rs       # Session state management
│   └── stealth.rs       # Stealth mode (anti-detection profiles)
├── config/
│   ├── mod.rs           # Configuration loading
│   └── profile.rs       # Profile management
└── commands/
    ├── mod.rs           # Command module exports
    ├── search.rs        # Search actions command
    ├── get.rs           # Get action by ID command
    ├── sources.rs       # List/search sources command
    ├── browser.rs       # Browser automation commands
    ├── config.rs        # Config management commands
    └── profile.rs       # Profile management commands
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
actionbook browser inspect <X> <Y>  # Inspect element at coordinates
actionbook browser viewport         # Show viewport size
actionbook browser connect <PORT>   # Connect to existing browser
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
| **Chrome Flags** | `--disable-blink-features=AutomationControlled`, `--disable-infobars` |
| **Persistent Injection** | Uses `Page.addScriptToEvaluateOnNewDocument` for cross-navigation persistence |

### Available Profiles

**OS**: `windows`, `macos-intel`, `macos-arm`, `linux`

**GPU**: `rtx4080`, `rtx3080`, `gtx1660`, `rx6800`, `uhd630`, `iris-xe`, `m1-pro`, `m2-max`, `m4-max`

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

- **54 tests** total
  - 42 CLI + unit tests (argument parsing, snapshot rendering)
  - 12 integration tests (API + CLI)

## License

MIT License - see [LICENSE](LICENSE) for details.

## Related

- [Actionbook MCP](../mcp) - MCP Server for AI agents
- [Actionbook SDK](../js-sdk) - JavaScript SDK
- [Actionbook API](https://api.actionbook.dev) - REST API

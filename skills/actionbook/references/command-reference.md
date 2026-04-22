# actionbook Command Reference

Complete reference for all `actionbook` CLI commands.

Every browser command requires `--session <SID>`. Most also require `--tab <TID>`.
Session-level commands (start, close, restart, status, list-sessions) need only `--session` or nothing.
Session IDs accept lowercase letters, digits, hyphens, and underscores (e.g., `s1`, `my-session`, `task_01`).

Selectors accept CSS, XPath, or snapshot refs (`@eN` from `snapshot` output).

## Global Flags

```
--json            Output as JSON envelope
--timeout <ms>    Command timeout in milliseconds
```

## Search

```bash
actionbook search "youtube"                                # Search for action manuals by keyword
actionbook search "youtube upload" --json                  # Search with JSON output
```

## Manual

```bash
actionbook manual youtube                                  # Overview of a site (groups & actions)
actionbook manual youtube videos                           # Actions in a group
actionbook manual youtube videos search                    # Detailed action documentation
actionbook manual youtube --json                           # JSON output
actionbook man youtube                                     # Alias for manual
```

## Session

```bash
actionbook browser start                                   # Start a browser session
actionbook browser start --set-session-id s1               # Get-or-create: reuse if Running, create if not (same as --session)
actionbook browser start --session s1                      # Get-or-create: reuse if exists, create if not
actionbook browser start --headless                        # Start headless
actionbook browser start --mode cloud --cdp-endpoint <ws>  # Connect to cloud browser
actionbook browser start -p hyperbrowser                   # Cloud provider (implies --mode cloud)
actionbook browser start -p driver --header "X-Key:val"    # Provider with custom CDP headers
actionbook browser start --open-url https://example.com    # Open URL on start
actionbook browser start --profile myprofile               # Use named profile
actionbook browser start --no-stealth                      # Disable anti-detection mode
actionbook browser start --max-tracked-requests 1000       # Custom network buffer size (default 500, range 1-100000)

actionbook browser list-sessions                           # List all active sessions (includes max_tracked_requests)
actionbook browser status --session s1                     # Show session status
actionbook browser close --session s1                      # Close a session (idempotent)
actionbook browser restart --session s1                    # Restart a session
```

`browser close` is **idempotent**: closing an unknown or already-closed session returns `ok: true` with `meta.warnings` instead of a fatal error. Envelope shape for an already-gone session:

- `ok: true`
- `data: { status: "closed", closed_tabs: 0 }`
- `meta.warnings: ["session not found in daemon — already closed or daemon restarted"]`

If another close is already in flight for the same session, the command returns `SESSION_CLOSING` (fatal, unchanged). Safe to call unconditionally during cleanup without checking session existence first. Read `meta.warnings` to distinguish a fresh close from an already-gone session.

Both `--session` and `--set-session-id` are get-or-create: they reuse a Running session with the given ID, or create one if not found. `--set-session-id` is a functional alias for `--session`. When reusing, if `--profile` is passed and does not match the session's bound profile, the command fails with `SESSION_PROFILE_MISMATCH` (retryable: false). Omitting `--profile` or passing a matching value allows reuse.

Supported cloud providers: `driver` (`DRIVER_API_KEY`), `hyperbrowser` (`HYPERBROWSER_API_KEY`), `browseruse` (`BROWSER_USE_API_KEY`). `-p` is mutually exclusive with `--cdp-endpoint` and `--mode local/extension`.

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
actionbook browser goto <url> --wait-until load --session s1 --tab t1   # Wait for full page load
actionbook browser goto <url> --wait-until none --session s1 --tab t1   # Return immediately
actionbook browser back --session s1 --tab t1              # Go back
actionbook browser forward --session s1 --tab t1           # Go forward
actionbook browser reload --session s1 --tab t1            # Reload page
```

`--wait-until` controls when `goto` returns: `domcontentloaded` (default), `load` (all resources), or `none` (immediate). A scheme (`https://`) is added automatically if omitted.

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
actionbook browser select "<selector>" @e12 --by-ref --session s1 --tab t1

When an option is not found, `select` returns structured diagnostics in the `details` field: available values, visible texts, current match mode (`by-value`/`by-text`), and total option count.

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
actionbook browser eval "await fetch('/api/data').then(r => r.json())" --no-isolate --session s1 --tab t1
actionbook browser eval --file script.js --session s1 --tab t1
echo 'document.title' | actionbook browser eval - --session s1 --tab t1
```

**eval input sources:** The expression comes from exactly one of three mutually-exclusive sources:
- **Positional argument** (default): `actionbook browser eval "expr" ...`
- **`--file <path>`**: read expression from a local file: `actionbook browser eval --file script.js ...`
- **Stdin (`-`)**: pipe the expression via stdin: `echo 'expr' | actionbook browser eval - ...`

Providing more than one source (or none) returns `EVAL_ARGS_CONFLICT`.

**eval scope isolation:** By default, `eval` wraps `let`/`const` declarations in an isolated scope so they don't leak across calls. Use `--no-isolate` to disable this — needed for multi-statement async expressions or when you want shared scope.

**eval response fields:** Success includes `pre_url`, `pre_origin`, `pre_readyState` (page state before execution) and `post_url`, `post_title` (page state after). On failure, `error.details` contains `{stage, pre_url, pre_origin, pre_readyState, error_type, reason}` plus optional `status`, `content_type`, and `body_head` (≤256 chars, UTF-8 boundary safe) for fetch-related errors.

**eval error codes:** On failure, `error.code` is one of:

| Code | When | Hint | `error.details` extras |
|------|------|------|------------------------|
| `EVAL_RUNTIME_ERROR` | JS exception (ReferenceError, TypeError, etc.) | Inspect the expression and referenced variables before retrying | `reason` |
| `EVAL_CROSS_ORIGIN` | Cross-origin fetch or SecurityError | Use same-origin fetch or proxy the request server-side | `reason` |
| `EVAL_RESPONSE_NOT_JSON` | `Content-Type` is not JSON when JSON was expected | Check content-type before parsing JSON | `reason`, `status`, `content_type`, `body_head` |
| `EVAL_RESPONSE_NOT_OK` | HTTP status is not 2xx | Handle non-2xx responses before decoding the body | `reason`, `status`, `content_type`, `body_head` |
| `EVAL_TIMEOUT` | Expression did not resolve within `--timeout` | Reduce work or raise --timeout | `reason` |
| `EVAL_ARGS_CONFLICT` | Multiple input sources, or no source at all | Provide exactly one of: positional expression, --file, or stdin (`-`) | `reason` |
| `EVAL_FILE_NOT_FOUND` | `--file` path unreadable (not found, permission denied, invalid data) | Verify --file points to a readable script path | `reason`, `path` |
| `EVAL_STDIN_TTY` | Positional `-` but stdin is a terminal (not piped) | Pipe the expression via stdin, e.g. `echo 'expr' \| actionbook browser eval -` | `reason` |
| `EVAL_STDIN_EMPTY` | Stdin read produced empty or whitespace-only input | Verify the upstream command or pipeline produces output | `reason` |

The first 5 codes are **runtime errors** (after CDP execution). The last 4 are **CLI-layer errors** (before any browser interaction) — they carry `details.stage` and `details.reason` but no page context (`pre_url`, `pre_origin`, etc.).

Read `error.code` to branch on the failure class. For `EVAL_RESPONSE_NOT_OK` and `EVAL_RESPONSE_NOT_JSON`, inspect `error.details.body_head` to distinguish 403 / challenge pages / CORS errors before deciding whether to retry. The `details.reason` field is an observability signal — branch on `error.code`, not on `details.reason`.

**fill vs type:** `fill` clears the field and sets the value directly (like pasting). `type` simulates individual keystrokes and appends to existing content.

**CDP error codes:** Browser commands that interact with elements, navigate, or communicate via CDP return structured error codes on failure. Branch on `error.code`:

| Code | When | Hint | Retryable | `error.details` extras |
|------|------|------|-----------|------------------------|
| `CDP_NODE_NOT_FOUND` | DOM node is stale or nonexistent | Call `actionbook browser snapshot` to refresh node references then retry | No | `reason`, `cdp_code` |
| `CDP_NOT_INTERACTABLE` | Element exists but can't be acted on (no box model) | Scroll it into view, wait for visibility, or dismiss overlays | No | `reason`, `cdp_code` |
| `CDP_NAV_TIMEOUT` | Navigation or eval timeout | Increase `--timeout` or verify the target URL is reachable | Yes | `reason`, `cdp_code`, `timeout_ms` |
| `CDP_TARGET_CLOSED` | CDP target closed mid-command (tab navigated away or session torn down) | Start a fresh session or re-attach to the tab | Yes | `reason`, `cdp_code` |
| `CDP_PROTOCOL_ERROR` | CDP response malformed or missing expected fields (`-32xxx` error codes) | Inspect `details.reason` and `details.cdp_code` for the raw protocol error | No | `reason`, `cdp_code` |
| `CDP_GENERIC` | CDP error that doesn't match any of the above (transport/parse) | *(no specific remediation)* | No | `reason` |

`CDP_NAV_TIMEOUT` and `CDP_TARGET_CLOSED` are retryable (`error.retryable == true` in the JSON envelope). All other CDP codes require caller intervention before retrying.

When `error.code` is a `CDP_*` code, `error.details` includes `reason` (raw CDP message) and `cdp_code` (upstream CDP numeric code, e.g. `-32000`) when available. Some sites include additional fields like `timeout_ms` for navigation timeouts.

**Legacy `CDP_ERROR`**: Some interaction paths (cookies, screenshots, PDF, etc.) still emit the legacy `CDP_ERROR` code. These are being migrated to the structured `CDP_*` taxonomy (ACT-999).

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
actionbook browser screenshot output.jpg --screenshot-format jpeg --session s1 --tab t1
actionbook browser screenshot output.png --selector "#main" --session s1 --tab t1  # Capture specific element
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

## Network

```bash
actionbook browser network requests --session s1 --tab t1                          # List all tracked requests
actionbook browser network requests --filter /api/ --session s1 --tab t1           # Filter by URL substring
actionbook browser network requests --type xhr,fetch --session s1 --tab t1         # Filter by resource type
actionbook browser network requests --method POST --session s1 --tab t1            # Filter by HTTP method
actionbook browser network requests --status 2xx --session s1 --tab t1             # Filter by status (200, 2xx, 400-499)
actionbook browser network requests --clear --session s1 --tab t1                  # Clear request buffer
actionbook browser network requests --dump --out /tmp/dump --session s1 --tab t1  # Export matching requests to /tmp/dump/requests.json
actionbook browser network requests --dump --out /tmp/dump --filter /api/ --session s1 --tab t1  # Export filtered requests

actionbook browser network request 1234.1 --session s1 --tab t1                   # Get full request detail + response body
```

Requests are captured automatically per tab (default 500, configurable via `browser start --max-tracked-requests N`). Use `network requests` to list IDs, then `network request <id>` for detail including response body.

`--dump --out <dir>` exports all matching requests (after filters) as a single `<dir>/requests.json` file with best-effort response bodies. Returns `dump: { path, count }` on success.

### HAR Recording

Record all network traffic for a tab in HAR 1.2 format.

```bash
actionbook browser network har start --session s1 --tab t1                        # Start recording
actionbook browser network har stop --session s1 --tab t1                         # Stop and export to ~/.actionbook/har/
actionbook browser network har stop --session s1 --tab t1 --out /tmp/trace.har    # Stop and export to custom path
```

Recording is per-tab: multiple tabs (or sessions) can record independently at the same time. `har start` accepts `--max-entries N` to set the ring-buffer cap (default: 10000). `har stop` writes a HAR 1.2 JSON file and returns `{ path, count, dropped, max_entries }`. If `--out` is omitted, a timestamped file is created in `~/.actionbook/har/`.

Output contains request/response headers, status, mimeType, and detailed timings per entry. Response bodies are not included — use `network requests --dump` if you need bodies. Redirect chains produce one entry per hop.

**Truncation signal**: When `har stop` completes and entries were dropped due to the ring-buffer cap (`dropped > 0`), the envelope includes:
- `meta.truncated == true`
- `meta.warnings` containing `"HAR_TRUNCATED: <N> earlier entries dropped (max_entries=<cap>); raise --max-entries or stop recording sooner to keep the full trace"`
- `data.max_entries` — the configured cap at stop time

On a clean stop (`dropped == 0`), `meta.truncated` is `false` and `meta.warnings` is empty.

Error codes: `HAR_ALREADY_RECORDING` (start while already recording on that tab), `HAR_NOT_RECORDING` (stop without a prior start). Recording data is held in memory; closing the tab while recording discards it. Cross-origin iframe requests are not captured (v1 limitation).

## Wait

```bash
actionbook browser wait element "<selector>" --session s1 --tab t1              # Wait for element
actionbook browser wait element "<selector>" --timeout 5000 --session s1 --tab t1
actionbook browser wait navigation --session s1 --tab t1                        # Wait for navigation
actionbook browser wait network-idle --session s1 --tab t1                      # Wait for network idle
actionbook browser wait condition "document.readyState === 'complete'" --session s1 --tab t1
```

Default timeout for all wait commands: 30000ms. Override with `--timeout <ms>`.

`wait network-idle` is edge-triggered: it only tracks fetch/XHR requests started after the command begins. Pre-existing background connections (SSE, WebSocket, in-flight fetches, analytics pings) are ignored and do not block. This is an agent-friendly settle signal, not a guarantee of global network silence.

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

## Batch

Batch commands operate on multiple targets in one call for higher throughput.

```bash
# Open multiple tabs
actionbook browser batch-new-tab --urls https://a.com https://b.com --session s1
actionbook browser batch-new-tab --urls https://a.com https://b.com --tabs inbox settings --session s1

# Snapshot multiple tabs
actionbook browser batch-snapshot --tabs t1 t2 t3 --session s1

# Click multiple elements sequentially
actionbook browser batch-click @e5 @e6 @e7 --session s1 --tab t1
```

`batch-new-tab` (alias `batch-open`) opens each URL as a new tab. If `--tabs` is provided, its length must match `--urls`. `batch-click` stops on first failure and reports progress. `batch-snapshot` returns per-tab results (ok or error).

## Extension

Manage the Chrome extension used by extension mode. The extension bridge runs inside the actionbook daemon (auto-started by browser commands).

The recommended install method is the [Chrome Web Store](https://chromewebstore.google.com/detail/actionbook/bebchpafpemheedhcdabookaifcijmfo) (current version: 0.4.0). `actionbook extension install` is a local fallback — after running it, you must manually load the unpacked extension in Chrome via `chrome://extensions` > Developer mode > Load unpacked, pointing to the path from `actionbook extension path`.

```bash
actionbook extension status                          # Bridge status + extension connection state
actionbook extension ping                            # Measure bridge RTT (connects to ws://localhost:19222)
actionbook extension install                         # Fallback: install to ~/Actionbook/extension/ (requires manual Chrome load)
actionbook extension install --force                 # Force reinstall even if up to date
actionbook extension uninstall                       # Remove extension from ~/Actionbook/extension/
actionbook extension path                            # Print install path, installed status, and version
```

`extension status` returns `bridge` state (`listening`, `not_listening`, or `failed`) and `extension_connected` (boolean). `extension ping` connects directly to the bridge WebSocket and measures round-trip time.

**Extension 0.4.0 changes:** Tabs opened by Actionbook are automatically grouped into a Chrome tab group titled "Actionbook" (toggleable via extension popup). In extension mode, `list-tabs` returns only Actionbook-managed tabs (debugger-attached or in the Actionbook tab group) — other user tabs are hidden. Local/cloud modes are unaffected. Extensions below 0.4.0 are rejected at handshake with a protocol mismatch error.

## Daemon

The actionbook daemon runs in the background and manages browser sessions. It auto-starts on first CLI call.

```bash
actionbook daemon restart                            # Stop the running daemon (next CLI call respawns)
```

## Setup

```bash
actionbook setup                                    # Interactive configuration wizard
actionbook setup --non-interactive --api-key <KEY>  # Non-interactive setup
actionbook setup --non-interactive --browser local   # Set browser mode non-interactively
actionbook setup --reset                            # Reset configuration
actionbook setup --target claude                    # Quick mode: install skills for an agent
actionbook setup -t codex                           # Short flag
# Targets: claude, codex, cursor, windsurf, antigravity, opencode, hermes, standalone, all
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

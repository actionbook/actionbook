---
name: actionbook
description: Browser action engine. Provides up-to-date action manuals for the modern web — operate any website instantly, one tab or dozens, concurrently.
version: 1.5.0
license: MIT
platforms: [macos, linux, windows]
metadata:
  hermes:
    tags: [browser-automation, web-automation, scraping, e2e-testing]
    requires_toolsets: [terminal]
required_environment_variables:
  - name: ACTIONBOOK_API_KEY
    prompt: "Actionbook API key"
    help: "Create one at https://actionbook.dev/dashboard — skill works without it, but requests are rate-limited"
    required_for: "unlimited requests (without a key, public rate limits apply)"
    optional: true
---

## When to Use This Skill

Activate when the user:
- Needs to do anything on a website ("Send a LinkedIn message", "Book an Airbnb", "Search Google for...")
- Asks how to interact with a site ("How do I post a tweet?", "How to apply on LinkedIn?")
- Wants to fill out forms, click buttons, navigate, search, filter, or browse on a specific site
- Wants to take a screenshot of a web page or monitor changes
- Builds browser-based AI agents, web scrapers, or E2E tests for external websites
- Automates repetitive web tasks (data entry, form submission, content posting)
- Needs to operate multiple websites or tabs concurrently

## How It Works

Actionbook provides **up-to-date action manuals** for the modern web. Action manuals tell agents exactly what to do on a page — no parsing, no guessing.

**Why this matters:**
- **10x faster** — action manuals provide selectors and page structure upfront. No snapshot-per-step loop needed.
- **Accurate** — handles SPAs, streaming components, dropdowns, date pickers, and dynamic content reliably.
- **Concurrent** — stateless architecture with explicit `--session`/`--tab`. Operate dozens of tabs in parallel.

The workflow:
1. **Start** a browser session
2. **Navigate** to the target page
3. **Snapshot** to get the page structure with element refs
4. **Automate** using refs from the snapshot

Run `actionbook <command> --help` for full usage and examples of any command.

## Browser Automation

Every browser command is **stateless** — pass `--session` and `--tab` explicitly. No "current tab" — you can run commands on any session/tab in parallel.

### Start a session

```bash
actionbook browser start --set-session-id s1
```

Both `--session` and `--set-session-id` are get-or-create: they reuse a Running session with the given ID, or create one if not found. If `--profile` is passed and does not match the session's bound profile, the command fails with `SESSION_PROFILE_MISMATCH`.

### Core workflow: snapshot, act, wait

```bash
actionbook browser goto <url> --session s1 --tab t1
actionbook browser snapshot --session s1 --tab t1          # Get page structure with refs
actionbook browser fill @e3 "text" --session s1 --tab t1   # Use refs from snapshot
actionbook browser click @e7 --session s1 --tab t1
actionbook browser wait navigation --session s1 --tab t1   # Wait for page load
```

### Snapshot refs

`snapshot` labels every element with a ref (e.g. `@e3`, `@e7`). Use these refs as selectors in any command — they are the recommended way to target elements.

Refs are **stable across snapshots** — if the element stays the same, the ref stays the same. This lets you chain multiple commands without re-snapshotting after every step.

### Command categories

All commands support `--help` for full usage and examples.

| Category | Key commands | Help |
|----------|-------------|------|
| Search | `search` | `actionbook search --help` |
| Manual | `manual` (alias: `man`) | `actionbook manual --help` |
| Session | `start`, `close`, `restart`, `list-sessions`, `status` | `actionbook browser start --help` |
| Tab | `new-tab`, `close-tab`, `list-tabs` | `actionbook browser new-tab --help` |
| Navigation | `goto`, `back`, `forward`, `reload` | `actionbook browser goto --help` |
| Observation | `snapshot`, `text`, `html`, `value`, `title`, `url`, `viewport`, `attr`, `attrs`, `box`, `styles`, `describe`, `state`, `inspect-point`, `screenshot`, `pdf` | `actionbook browser snapshot --help` |
| Interaction | `click`, `fill`, `type`, `press`, `select`, `hover`, `focus`, `scroll`, `drag`, `upload`, `eval`, `mouse-move`, `cursor-position` | `actionbook browser click --help` |
| Wait | `wait element`, `wait navigation`, `wait network-idle`, `wait condition` | `actionbook browser wait element --help` |
| Cookies | `cookies list`, `cookies get`, `cookies set`, `cookies delete`, `cookies clear` | `actionbook browser cookies list --help` |
| Storage | `local-storage list\|get\|set\|delete\|clear`, `session-storage ...` | `actionbook browser local-storage get --help` |
| Logs | `logs console`, `logs errors` | `actionbook browser logs console --help` |
| Network | `network requests`, `network request <id>`, `network har start`, `network har stop` | `actionbook browser network requests --help` |
| Query | `query one\|all\|nth\|count` | `actionbook browser query --help` |
| Batch | `batch-new-tab`, `batch-snapshot`, `batch-click` | `actionbook browser batch-new-tab --help` |
| Extension | `extension status`, `extension ping`, `extension install`, `extension uninstall`, `extension path` | `actionbook extension status --help` |
| Daemon | `daemon restart` | `actionbook daemon restart --help` |

Full command reference: [command-reference.md](references/command-reference.md)

### Cloud providers

Use `-p` / `--provider` with `browser start` to run sessions on a remote browser instead of launching local Chrome. Supported providers: `driver`, `hyperbrowser`, `browseruse`. Each reads its own `<PROVIDER>_API_KEY` from the shell env.

```bash
export HYPERBROWSER_API_KEY="your-key"
actionbook browser start -p hyperbrowser --session s1
actionbook browser goto "https://example.com" --session s1 --tab t1
actionbook browser snapshot --session s1 --tab t1
```

All browser commands work the same way regardless of mode. `browser restart --session <id>` mints a fresh remote session while preserving the session_id.

## Example: End-to-End

User request: "Find a room next week in SF on Airbnb"

```bash
actionbook browser start --set-session-id s1
actionbook browser goto "https://airbnb.com" --session s1 --tab t1
actionbook browser snapshot --session s1 --tab t1
actionbook browser fill @e3 "San Francisco" --session s1 --tab t1
actionbook browser click @e7 --session s1 --tab t1
actionbook browser wait navigation --session s1 --tab t1
```

## Eval Input Sources

`browser eval` accepts the expression from three mutually-exclusive sources:
- **Positional**: `actionbook browser eval "expr" ...`
- **`--file`**: `actionbook browser eval --file script.js ...`
- **Stdin**: `echo 'expr' | actionbook browser eval - ...`

## Eval Error Handling

`browser eval` returns structured error codes on failure — branch on `error.code` instead of parsing the message:

- `EVAL_RUNTIME_ERROR` — JS exception. Inspect the expression before retrying.
- `EVAL_CROSS_ORIGIN` — cross-origin fetch or CSP block. Proxy the request server-side.
- `EVAL_RESPONSE_NOT_JSON` / `EVAL_RESPONSE_NOT_OK` — read `error.details.body_head` (first ≤256 chars of the response body) to distinguish 403 / challenge pages / CORS errors. Do not blindly retry.
- `EVAL_TIMEOUT` — expression exceeded `--timeout`. Reduce work or raise the timeout.
- `EVAL_ARGS_CONFLICT` — multiple input sources or none. Provide exactly one.
- `EVAL_FILE_NOT_FOUND` — `--file` path unreadable. Verify the path.
- `EVAL_STDIN_TTY` — `-` but stdin is a terminal. Pipe the expression.
- `EVAL_STDIN_EMPTY` — stdin produced empty input. Verify the upstream pipeline.

## CDP Error Handling

Browser commands that interact with elements, navigate, or communicate via CDP return structured error codes — branch on `error.code`:

- `CDP_NODE_NOT_FOUND` — DOM node is stale. Call `snapshot` to refresh refs then retry.
- `CDP_NOT_INTERACTABLE` — element exists but can't be acted on. Scroll into view, wait for visibility, or dismiss overlays.
- `CDP_NAV_TIMEOUT` — navigation timeout. Increase `--timeout` or verify URL reachability. **Retryable.**
- `CDP_TARGET_CLOSED` — tab navigated away or session torn down mid-command. Start a fresh session. **Retryable.**
- `CDP_PROTOCOL_ERROR` — CDP response malformed. Inspect `details.reason` and `details.cdp_code`.
- `CDP_GENERIC` — unclassified CDP error (transport/parse). No specific remediation.

`CDP_NAV_TIMEOUT` and `CDP_TARGET_CLOSED` are retryable (`error.retryable == true`). All other CDP codes require caller intervention before retrying. When `error.code` is a `CDP_*` code, `error.details` includes `reason` and `cdp_code` when available.

## Selectors

Selectors should come from `actionbook browser snapshot` — not from prior knowledge or memory. Always snapshot first to get current refs, then use those refs to interact with the page.

## Login Page Handling

When you hit a login/auth wall (sign-in page, password prompt, MFA/OTP, CAPTCHA, account chooser):

1. **Pause automation and keep the current browser session open** (same tab/profile/cookies).
2. **Ask the user to complete login manually** in that same browser window.
3. After user confirms login is done, **continue in the same session**.
4. If the post-login page is different, run `actionbook browser snapshot` to get the new page structure before continuing.

Do not switch tools just because a login page appears.

## Session Cleanup

`browser close` is idempotent — closing an unknown or already-closed session returns `ok: true` with a warning in `meta.warnings`, not a fatal error. A typo in the session ID or a session that was already torn down is no longer an error condition.

- Safe to call `browser close` unconditionally during cleanup without checking session existence first.
- Read `meta.warnings` to distinguish a fresh close from an already-gone session. Do not treat a warning inside an `ok: true` response as a signal that the session is still alive.
- If another close is already in flight for the same session, the command returns `SESSION_CLOSING` (fatal).

## HAR Recording

`network har start` accepts `--max-entries N` to set the ring-buffer cap (default: 10000). When `har stop` detects dropped entries (`data.dropped > 0`), the envelope includes `meta.truncated = true` and a `HAR_TRUNCATED` warning in `meta.warnings`. Read `data.max_entries` to see the configured cap. Raise `--max-entries` or stop recording sooner to keep the full trace.

## References

| Reference | Description |
|-----------|-------------|
| [command-reference.md](references/command-reference.md) | Complete command reference with all flags and options |
| [authentication.md](references/authentication.md) | Login flows, OAuth, 2FA handling, session persistence |

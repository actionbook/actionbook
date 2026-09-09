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
    help: "Create one at https://actionbook.app/dashboard — skill works without it, but requests are rate-limited"
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
1. **Classify ownership** before mutating a session.
2. **Start or address** the owned session/tab explicitly.
3. **Navigate and snapshot** to get current element refs.
4. **Automate** using refs from the snapshot.
5. **Always run ownership-specific cleanup** in a finally-style terminal path.

Run `actionbook <command> --help` for full usage and examples of any command.

## Ownership and Mandatory Terminal Cleanup

Before the first browser mutation, classify the resource and record every tab this task creates. Cleanup is mandatory on success, failure, cancellation, timeout, and abandonment:

- **Dedicated persistent/authenticated named profile owned by this task:** always stop its local Chrome while retaining authentication state.
  ```bash
  actionbook browser stop --session "$SESSION"
  ```
- **Dedicated temporary profile owned by this task:** use destructive close only when deleting that profile is intended.
  ```bash
  actionbook browser close --session "$SESSION"
  ```
- **Shared session (for example `lzero-default`):** never stop or close the session. Close every tab created by this task, and only those tabs.
  ```bash
  actionbook browser close-tab --session lzero-default --tab "$OWNED_TAB_1"
  actionbook browser close-tab --session lzero-default --tab "$OWNED_TAB_2"
  ```
- **Unknown ownership:** do not stop, close, or close tabs. Report the residual session/tab IDs and request an owner decision.

Install best-effort cleanup before acquiring a dedicated resource, not at the happy-path end. Preserve normal/failure status and re-raise SIGINT or SIGTERM after cleaning:

```bash
set -Eeuo pipefail
SESSION=research-task
PROFILE=research-auth
cleanup() { actionbook browser stop --session "$SESSION" >/dev/null 2>&1 || true; }
trap 'status=$?; trap - EXIT INT TERM; cleanup; exit "$status"' EXIT
trap 'trap - EXIT INT TERM; cleanup; kill -INT "$$"' INT
trap 'trap - EXIT INT TERM; cleanup; kill -TERM "$$"' TERM

actionbook browser start --session "$SESSION" --profile "$PROFILE"
# ... task work; cleanup runs on exit, command failure, SIGINT, or SIGTERM ...
```

A login/manual wait may retain resources only when the current owner and a concrete resume condition are named (for example, “task `research-task` owns session `research-task`; resume when the user confirms MFA completed”). If that wait is abandoned or becomes terminal, run the same ownership-specific cleanup.

## Browser Automation

Every browser command is **stateless** — pass `--session` and `--tab` explicitly. No "current tab" — you can run commands on any session/tab in parallel.

### Start a session

Acquire dedicated sessions with cleanup already installed:

```bash
set -Eeuo pipefail
SESSION=s1
PROFILE=s1-auth
cleanup() { actionbook browser stop --session "$SESSION" >/dev/null 2>&1 || true; }
trap 'status=$?; trap - EXIT INT TERM; cleanup; exit "$status"' EXIT
trap 'trap - EXIT INT TERM; cleanup; kill -INT "$$"' INT
trap 'trap - EXIT INT TERM; cleanup; kill -TERM "$$"' TERM
actionbook browser start --session "$SESSION" --profile "$PROFILE"
```

Both `--session` and `--set-session-id` are get-or-create: they reuse a Running session with the given ID, or create one if not found. If `--profile` is passed and does not match the session's bound profile, the command fails with `SESSION_PROFILE_MISMATCH`. Shared-session work does not call start; it creates owned tabs in the existing shared session and closes those tabs in its terminal cleanup.

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
| Session | `start`, `stop`, `close`, `restart`, `list-sessions`, `status` | `actionbook browser start --help` |
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
set -Eeuo pipefail
export HYPERBROWSER_API_KEY="your-key"
SESSION=cloud-s1
cleanup() { actionbook browser close --session "$SESSION" >/dev/null 2>&1 || true; }
trap 'status=$?; trap - EXIT INT TERM; cleanup; exit "$status"' EXIT
trap 'trap - EXIT INT TERM; cleanup; kill -INT "$$"' INT
trap 'trap - EXIT INT TERM; cleanup; kill -TERM "$$"' TERM
actionbook browser start -p hyperbrowser --session "$SESSION"
actionbook browser goto "https://example.com" --session "$SESSION" --tab t1
actionbook browser snapshot --session "$SESSION" --tab t1
```

All browser commands work the same way regardless of mode. `browser restart --session <id>` mints a fresh remote session while preserving the session_id.

## Example: End-to-End

User request: "Find a room next week in SF on Airbnb"

```bash
set -Eeuo pipefail
SESSION=airbnb-task
PROFILE=airbnb-auth
cleanup() { actionbook browser stop --session "$SESSION" >/dev/null 2>&1 || true; }
trap 'status=$?; trap - EXIT INT TERM; cleanup; exit "$status"' EXIT
trap 'trap - EXIT INT TERM; cleanup; kill -INT "$$"' INT
trap 'trap - EXIT INT TERM; cleanup; kill -TERM "$$"' TERM

actionbook browser start --session "$SESSION" --profile "$PROFILE"
actionbook browser goto "https://airbnb.com" --session "$SESSION" --tab t1
actionbook browser snapshot --session "$SESSION" --tab t1
actionbook browser fill @e3 "San Francisco" --session "$SESSION" --tab t1
actionbook browser click @e7 --session "$SESSION" --tab t1
actionbook browser wait navigation --session "$SESSION" --tab t1
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

1. **Pause automation and keep the current browser session open** only when its current owner and resume condition are explicit.
2. **Ask the user to complete login manually** in that same browser window and name the session/tab to resume.
3. After user confirms the named condition is met, **continue in the same session**.
4. If the post-login page is different, run `actionbook browser snapshot` to get the new page structure before continuing.
5. If the wait is abandoned, cancelled, or terminal, run ownership-specific cleanup immediately.

Do not switch tools just because a login page appears, and do not leave ownerless resources waiting indefinitely.

## Session Cleanup

`browser stop` releases an Actionbook-owned local Chrome and preserves its named profile. `browser close` is the explicit destructive operation for non-default local profiles. Both belong in the mandatory terminal cleanup described above.

- Stop/close are idempotent when the session and profile-scoped ownership record are already gone; stop returns null profile fields rather than claiming preservation in that case.
- A crash-orphan is killed only after the authoritative ownership record, session identity, PID start identity, command line, and profile path prove Actionbook ownership. Ownership mismatch fails visibly.
- Read `meta.warnings` and structured profile fields to distinguish a fresh teardown from an already-gone session.
- If another teardown is already in flight, the command returns `SESSION_CLOSING` (fatal).
- Shared sessions such as `lzero-default` are never stopped or closed; their task-created tabs are closed individually.

## HAR Recording

`network har start` accepts `--max-entries N` to set the ring-buffer cap (default: 10000). When `har stop` detects dropped entries (`data.dropped > 0`), the envelope includes `meta.truncated = true` and a `HAR_TRUNCATED` warning in `meta.warnings`. Read `data.max_entries` to see the configured cap. Raise `--max-entries` or stop recording sooner to keep the full trace.

## References

| Reference | Description |
|-----------|-------------|
| [command-reference.md](references/command-reference.md) | Complete command reference with all flags and options |
| [authentication.md](references/authentication.md) | Login flows, OAuth, 2FA handling, session persistence |

---
name: actionbook
description: Browser action engine. Provides up-to-date action manuals for the modern web — operate any website instantly, one tab or dozens, concurrently.
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
1. **Search** for action manuals for the target website
2. **Get** the action details (selectors, page structure)
3. **Start** a browser session
4. **Automate** using selectors from the action manual or from live snapshots

Run `actionbook <command> --help` for full usage and examples of any command.

## Step 1: Search and Get

### search — Find actions by task description

```bash
actionbook search "<query>"                      # Search by task intent
actionbook search "<query>" --domain site.com    # Filter by domain
actionbook search "<query>" --url <url>          # Filter by URL
```

**Returns** area IDs with descriptions and relevance scores. Use the area_id with `actionbook get` to fetch full details.

### Constructing an effective search query

The `query` string is the **primary signal** for finding the right action. Pack it with the user's full intent — not just a site name or a vague keyword.

**Include in the query:**
1. **Target site** — the website name or domain
2. **Task verb** — what the user wants to do (search, book, post, filter, login, compose, etc.)
3. **Object / context** — what they're acting on (listings, messages, flights, repositories, etc.)
4. **Specific details** — any constraints, filters, or parameters the user mentioned

**Rule of thumb:** Rewrite the user's request as a single descriptive sentence and use that as the query.

| User says | Bad query | Good query |
|-----------|-----------|------------|
| "Book an Airbnb in Tokyo for next week" | `"airbnb"` | `"airbnb search listings Tokyo dates check-in check-out guests"` |
| "Search arXiv for recent NLP papers" | `"arxiv search"` | `"arxiv advanced search papers NLP natural language processing recent"` |
| "Send a LinkedIn connection request" | `"linkedin"` | `"linkedin send connection request invite someone"` |

If `--domain` or `--url` is known, always add them — they narrow results and improve precision.

### get — Retrieve full action details by ID

```bash
actionbook get "arxiv.org:/search/advanced:default"
```

**Returns** a structured action manual with page URL, overview, function summary, and element selectors. Use the selectors from the action manual in browser commands.

## Step 2: Browser Automation

Every browser command is **stateless** — pass `--session` and `--tab` explicitly. No "current tab" — you can run commands on any session/tab in parallel.

### Start a session

```bash
actionbook browser start --set-session-id s1
```

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
| Session | `start`, `close`, `restart`, `list-sessions`, `status` | `actionbook browser start --help` |
| Tab | `new-tab`, `close-tab`, `list-tabs` | `actionbook browser new-tab --help` |
| Navigation | `goto`, `back`, `forward`, `reload` | `actionbook browser goto --help` |
| Observation | `snapshot`, `text`, `html`, `value`, `screenshot`, `title`, `url` | `actionbook browser snapshot --help` |
| Interaction | `click`, `fill`, `type`, `press`, `select`, `hover`, `scroll` | `actionbook browser click --help` |
| Wait | `wait element`, `wait navigation`, `wait network-idle`, `wait condition` | `actionbook browser wait element --help` |
| Cookies | `cookies list`, `cookies get`, `cookies set`, `cookies delete`, `cookies clear` | `actionbook browser cookies list --help` |
| Storage | `local-storage list\|get\|set\|delete\|clear`, `session-storage ...` | `actionbook browser local-storage get --help` |
| Logs | `logs console`, `logs errors` | `actionbook browser logs console --help` |
| Query | `query one\|all\|nth\|count` | `actionbook browser query --help` |

Full command reference: [command-reference.md](references/command-reference.md)

## Example: End-to-End

User request: "Find a room next week in SF on Airbnb"

```bash
# 1. Search for pre-verified actions
actionbook search "find a room next week in SF on airbnb" --domain airbnb.com

# 2. Get action details with selectors
actionbook get "airbnb.com:/:default"

# 3. Automate
actionbook browser start --set-session-id s1
actionbook browser goto "https://airbnb.com" --session s1 --tab t1
actionbook browser snapshot --session s1 --tab t1
actionbook browser fill @e3 "San Francisco" --session s1 --tab t1
actionbook browser click @e7 --session s1 --tab t1
actionbook browser wait navigation --session s1 --tab t1
```

## Fallback: Live Snapshots

Actionbook stores page data captured at indexing time. Websites evolve, so selectors may become outdated.

When a selector from `actionbook get` fails at runtime, use `actionbook browser snapshot` — it provides the **live page structure** with current refs. Use refs from the snapshot output to retry the interaction.

If `actionbook search` returns no results for a page, use `snapshot` as the primary source.

Selectors should come from `actionbook get` or `actionbook browser snapshot` — not from prior knowledge or memory.

## Login Page Handling

When you hit a login/auth wall (sign-in page, password prompt, MFA/OTP, CAPTCHA, account chooser):

1. **Pause automation and keep the current browser session open** (same tab/profile/cookies).
2. **Ask the user to complete login manually** in that same browser window.
3. After user confirms login is done, **continue in the same session**.
4. If the post-login page is a different type, run `actionbook search` + `actionbook get` for that new page before continuing.

Do not switch tools just because a login page appears.

## References

| Reference | Description |
|-----------|-------------|
| [command-reference.md](references/command-reference.md) | Complete command reference with all flags and options |
| [authentication.md](references/authentication.md) | Login flows, OAuth, 2FA handling, session persistence |

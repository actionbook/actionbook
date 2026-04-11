# Actionbook CLI v1.0.0 — API Reference

> Source document: `cli_prd.md`
> Date: 2026-03-26

---

## Table of Contents

**Part I — Principles & Conventions**

1. [Design Goals & Principles](#1-design-goals--principles)
2. [General Conventions](#2-general-conventions)
3. [Error Protocol](#3-error-protocol)
4. [Compatibility & Boundary Rules](#4-compatibility--boundary-rules)
5. [Test Scenarios](#5-test-scenarios)

**Part II — Detailed Command Definitions**

6. [Non-Browser Commands](#6-non-browser-commands)
7. [Browser Lifecycle](#7-browser-lifecycle)
8. [Browser Tab Management](#8-browser-tab-management)
9. [Browser Navigation](#9-browser-navigation)
10. [Browser Observation](#10-browser-observation)
11. [Browser Interaction](#11-browser-interaction)
12. [Browser Waiting](#12-browser-waiting)
13. [Browser Cookies](#13-browser-cookies)
14. [Browser Storage](#14-browser-storage)

---

## 1. Design Goals & Principles

### 1.1 Design Goals

- All commands return a unified structure
- Session-scoped commands always explicitly carry `session_id` and `tab_id`
- Default text output also has a stable format, no longer just "best-effort readable"
- This is an entirely new protocol version, not backward-compatible with the legacy response format

### 1.2 Design Principles

1. `--json` always uses a unified envelope
2. Text output always uses a fixed header format
3. Session context is a first-class concept, not hidden in logs
4. Non-session commands do not return `context`
5. `session_id` supports semantic naming
6. `tab_id` uses short IDs, suitable for humans and LLMs to reuse in the terminal

### 1.3 ID Conventions

#### session_id

- **Type:** String
- **Auto-generated format:** `sN` (e.g. `s1`, `s2`, `s3`) — global counter, mode-agnostic
- **Manual format:** via `actionbook browser start --set-session-id <SID>` — must match `^[a-z][a-z0-9-]{1,63}$`
- **Requirements:**
  - Uniquely identifies a session within the current CLI lifecycle and persisted state
- **Examples (manual):** `research-google`, `github-login-debug`, `airbnb-form-fill`

#### tab_id

- **Type:** Short string
- **Format:** `t1`, `t2`, `t3`, ...
- **Scope:** Unique within a single session
- **Notes:**
  - `tab_id` is the external-facing ID
  - If the underlying browser's native tab handle is needed, it can be additionally exposed via `native_tab_id`
  - Users are not allowed to manually name tabs

#### Optional Internal IDs

If the underlying bridge requires it, the JSON may include:
- `native_tab_id`
- `native_window_id`

These fields are only used for bridging and debugging, not as primary reference IDs.

### 1.4 Default Decisions

This version adopts the following defaults:

- `session_id` auto-generated as `sN` (s1, s2, …); user-specifiable via `--set-session-id`
- `tab_id` is a session-scoped short ID `tN`
- `native_tab_id` is an optional debug-only field
- Non-session commands omit `context`
- Both JSON and text output are part of the formal contract
- Unified envelope; does not continue the legacy response format

---

## 2. General Conventions

### 2.1 Definitions

| Symbol | Meaning |
|------|------|
| `<selector>` | ref (`@eN`), CSS selector, or XPath |
| `<coordinates>` | Coordinates in `x,y` format |
| `<SID>` | Session ID, a semantic string (e.g., `research-google`) |
| `<TID>` | Tab ID, short ID format `tN` (e.g., `t1`, `t2`) |
| `<WID>` | Window ID, short ID format `wN` (e.g., `w0`, `w1`) |

### 2.2 Global Flags

All `browser` subcommands support:

| Flag | Type | Description |
|------|------|------|
| `--timeout <ms>` | u64 | Timeout in milliseconds |
| `--json` | bool | JSON output (default is plain text) |

### 2.3 Addressing Levels

| Level | Requirements | Examples |
|------|------|------|
| **Global** | No session/tab | `browser start`, `browser list-sessions` |
| **Session** | `--session <SID>` | `browser status`, `browser list-tabs`, `cookies *` |
| **Tab** | `--session <SID> --tab <TID>` | `browser goto`, `browser click`, `storage *` |

### 2.4 JSON Envelope (Unified Response Format)

```json
{
  "ok": true,
  "command": "browser snapshot",
  "context": {
    "session_id": "research-google",
    "tab_id": "t1",
    "window_id": "w1",
    "url": "https://google.com",
    "title": "Google"
  },
  "data": {},
  "error": null,
  "meta": {
    "duration_ms": 123,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}
```

**Top-level fields:**

| Field | Type | Required | Description |
|------|------|------|------|
| `ok` | boolean | Yes | Whether the command succeeded |
| `command` | string | Yes | Normalized command name (e.g., `browser snapshot`) |
| `context` | object/null | No | Returned only for session commands |
| `data` | any/null | Yes | Business return value (typically an object; string for help/version; `null` on failure) |
| `error` | object/null | Yes | Error information (`null` on success) |
| `meta` | object | Yes | Metadata |

**context field rules:**
- `session_id`: Required for session commands
- `tab_id`: Required for tab-level commands
- `window_id`: Returned only in multi-window scenarios
- `url` / `title`: Returned when known in the current context
- Special case: `browser start` is a Global-level command (no session input required), but returns `context` after creating a session (containing the newly created session_id and tab_id)

**error structure:**

```json
{
  "code": "ELEMENT_NOT_FOUND",
  "message": "Element not found: #submit",
  "retryable": false,
  "details": { "selector": "#submit" }
}
```

**meta structure:**

```json
{
  "duration_ms": 123,
  "warnings": [],
  "pagination": { "page": 1, "page_size": 10, "total": 42, "has_more": true },
  "truncated": false
}
```

### 2.5 Text Output Protocol

**Non-session commands:** First line has no prefix; output directly.

**Session-level commands:**
```
[<session_id>]
<body>
```

**Tab-level commands:**
```
[<session_id> <tab_id>] <url>
<body>
```

**General rules:**
- Read commands output the body directly
- Action commands start with `ok <command>`
- Failures always output `error <CODE>: <message>`
- Note: Some commands' text output may deviate from the strict format (e.g., `restart` is Session-level but output includes tab_id, `close-tab` is Tab-level but omits URL) — these follow the PRD examples as the source of truth

---

## 3. Error Protocol

### 3.1 Unified Error Response

```json
{
  "ok": false,
  "command": "browser click",
  "context": { "session_id": "research-google", "tab_id": "t1", "url": "https://google.com" },
  "data": null,
  "error": {
    "code": "ELEMENT_NOT_FOUND",
    "message": "Element not found: button[type=submit]",
    "retryable": false,
    "details": { "selector": "button[type=submit]" }
  },
  "meta": { "duration_ms": 3012, "warnings": [], "pagination": null, "truncated": false }
}
```

**Text output:**
```
[research-google t1] https://google.com
error ELEMENT_NOT_FOUND: Element not found: button[type=submit]
```

### 3.2 Recommended Error Codes

| Error Code | Description |
|--------|------|
| `INVALID_ARGUMENT` | Invalid argument |
| `SESSION_NOT_FOUND` | Session does not exist |
| `TAB_NOT_FOUND` | Tab does not exist |
| `FRAME_NOT_FOUND` | Frame does not exist |
| `ELEMENT_NOT_FOUND` | Element does not exist |
| `MULTIPLE_MATCHES` | `query one` matched more than 1 |
| `INDEX_OUT_OF_RANGE` | `query nth` index out of range |
| `TIMEOUT` | Operation timed out |
| `NAVIGATION_FAILED` | Navigation failed |
| `EVAL_FAILED` | JavaScript execution failed |
| `ARTIFACT_WRITE_FAILED` | File write failed (screenshot/pdf) |
| `UNSUPPORTED_OPERATION` | Unsupported operation |
| `INTERNAL_ERROR` | Internal error |

---

## 4. Compatibility & Boundary Rules

- Non-session commands must omit `context`
- Session commands must return `context.session_id` as long as the session has been located
- Tab commands must return `context.tab_id` as long as the tab has been located
- `data` must always be present; `null` on failure
- `error` must always be present; `null` on success
- `command` uses a normalized name, not the raw argv string
- Text output must not include implementation detail logs
- Default text output and JSON must be semantically consistent

---

## 5. Test Scenarios

### 5.1 Basic Consistency

- `search` / `get` / `setup` / `help` / `--version` do not return `context`
- Browser commands always return `session_id` when applicable
- Tab-level commands always return `tab_id`

### 5.2 ID Rules

- Without `--set-session-id`, the first session is `s1`, the second `s2`, etc. (global counter)
- `browser start --set-session-id research-google` returns `session_id` = `"research-google"`
- Opening tabs sequentially within the same session returns `t1`, `t2`, `t3`
- `native_tab_id` may change, but `tab_id` remains stable for the upper layer

### 5.3 Text Format

- Snapshot body must not contain extraneous prompt text
- `text` / `html` / `eval` body outputs the value directly
- `click` / `fill` / `upload` / `wait` second line always starts with `ok <command>`
- Errors always follow the format `error <CODE>: <message>`

### 5.4 JSON Structure

- All commands have `ok` / `command` / `data` / `error` / `meta`
- Pagination information only appears in `meta.pagination`
- Truncation information only appears in `meta.truncated`

### 5.5 Query Semantics

- `browser query one` succeeds only when there is exactly 1 match
- `browser query one` returns `MULTIPLE_MATCHES` when there are more than 1 match
- `browser query one` returns `ELEMENT_NOT_FOUND` when there are 0 matches
- `browser query nth <n>` uses 1-based indexing
- `browser query nth <n>` returns `INDEX_OUT_OF_RANGE` when `n > count`
- `browser query all` succeeds with 0 matches, returning an empty array
- `browser query count` succeeds with 0 matches, returning only `count`

### 5.6 Navigation & Context Updates

- `goto` / `click` / `back` / `forward` / `reload` update `context.url` after success
- `context.title` is updated synchronously when the page title is known

---

## 6. Non-Browser Commands

These commands do not return `context`.

### 6.1 `actionbook search <query>`

Search for actions.

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `<query>` | string | Yes | Search keywords |
| `-d, --domain` | string | No | Filter by domain |
| `-u, --url` | string | No | Filter by URL |
| `-p, --page` | int | No | Page number, default 1 |
| `-s, --page-size` | int | No | Items per page, default 10 |

**JSON `data`:**

```json
{
  "query": "google login",
  "items": [
    {
      "area_id": "google.com:/login:default",
      "title": "Google Login",
      "summary": "Login form and related actions",
      "score": 0.98,
      "url": "https://google.com/login"
    }
  ]
}
```

**Text output:**
```
1 result
1. google.com:/login:default
   Google Login
   score: 0.98
   https://google.com/login
```

---

### 6.2 `actionbook get <area_id>`

Get action details.

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `<area_id>` | string | Yes | Action area ID |

**JSON `data`:**

```json
{
  "area_id": "google.com:/login:default",
  "url": "https://google.com/login",
  "description": "Login page",
  "elements": [
    {
      "element_id": "email",
      "type": "input",
      "description": "Email input",
      "css": "#identifierId",
      "xpath": null,
      "allow_methods": ["fill", "type", "focus"]
    }
  ]
}
```

**Text output:**
```
google.com:/login:default
https://google.com/login

Login page

[email] input
description: Email input
css: #identifierId
methods: fill, type, focus
```

---

### 6.3 `actionbook setup`

Interactive configuration wizard.

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `--target` | string | No | Quick mode only: install skills for the specified agent and skip the setup wizard. Conflicts with `--api-key`, `--browser`, and `--reset`. |
| `--api-key` | string | No | API Key |
| `--browser` | string | No | Browser configuration |
| `--non-interactive` | bool | No | Non-interactive mode |
| `--reset` | bool | No | Reset configuration |

> Setup enters an interactive configuration flow. The PRD does not define a JSON/text return value protocol (interactive commands do not output via `--json`). Return values for non-interactive mode (`--non-interactive`) are pending PRD clarification.

---

### 6.4 `actionbook help`

**JSON `data`:** `"help text here"` (string)

**Text output:**
```
actionbook browser <subcommand>

start      Start or attach a browser session
list-tabs  List tabs in a session
snapshot   Capture accessibility snapshot
```

---

### 6.5 `actionbook --version`

**JSON `data`:** `"1.0.0"` (string)

**Text output:** `1.0.0`

---

## 7. Browser Lifecycle

### 7.1 `actionbook browser start`

> Addressing level: **Global**
> command: `browser start`

Create or attach to a session. The session_id can be specified via `--set-session-id`; optionally auto-opens an initial tab.

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `--mode` | `local\|extension\|cloud` | No | Browser mode, default `local` |
| `--headless` | bool | No | Whether to use headless mode |
| `--profile` | string | No | Profile to use for the session |
| `--open-url` | string | No | Navigate to this URL when opening the browser |
| `--cdp-endpoint` | string | No | Connect to an existing CDP endpoint (does not launch a new browser) |
| `--header <KEY:VALUE>` | string | No | Only effective with `--cdp-endpoint`, passes headers when connecting |
| `--set-session-id` | string | No | Specify a semantic session ID |

**JSON `data`:**

```json
{
  "session": {
    "session_id": "research-google",
    "mode": "local",
    "status": "running",
    "headless": false,
    "cdp_endpoint": "ws://127.0.0.1:9222/devtools/browser/..."
  },
  "tab": {
    "tab_id": "t1",
    "url": "https://google.com",
    "title": "Google",
    "native_tab_id": 391
  },
  "reused": false
}
```

**Text output:**
```
[research-google t1] https://google.com
ok browser start
mode: local
status: running
title: Google
```

---

### 7.2 `actionbook browser list-sessions`

> Addressing level: **Global**
> command: `browser list-sessions`

List all active sessions.

**Parameters:** None

**JSON `data`:**

```json
{
  "total_sessions": 1,
  "sessions": [
    {
      "session_id": "research-google",
      "mode": "local",
      "status": "running",
      "headless": false,
      "tabs_count": 2
    }
  ]
}
```

**Text output:**
```
1 session
[research-google]
status: running
tabs: 2
```

---

### 7.3 `actionbook browser status --session <SID>`

> Addressing level: **Session**
> command: `browser status`

View detailed status of a specified session.

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `--session <SID>` | string | Yes | Session ID |

**JSON `data`:**

```json
{
  "session": {
    "session_id": "research-google",
    "mode": "local",
    "status": "running",
    "headless": false,
    "tabs_count": 2
  },
  "tabs": [
    {
      "tab_id": "t1",
      "url": "https://google.com",
      "title": "Google"
    }
  ],
  "capabilities": {
    "snapshot": true,
    "pdf": true,
    "upload": true
  }
}
```

**Text output:**
```
[research-google]
status: running
mode: local
tabs: 2
```

---

### 7.4 `actionbook browser close --session <SID>`

> Addressing level: **Session**
> command: `browser close`

Close the specified session and its browser.

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `--session <SID>` | string | Yes | Session ID |

**JSON `data`:**

```json
{
  "session_id": "research-google",
  "status": "closed",
  "closed_tabs": 2
}
```

**Text output:**
```
[research-google]
ok browser close
closed_tabs: 2
```

---

### 7.5 `actionbook browser restart --session <SID>`

> Addressing level: **Session**
> command: `browser restart`

Close and restart the session with the same profile/mode.

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `--session <SID>` | string | Yes | Session ID |

**JSON `data`:**

```json
{
  "session": {
    "session_id": "research-google",
    "mode": "local",
    "status": "running",
    "headless": false,
    "tabs_count": 1
  },
  "reopened": true
}
```

**Text output:**
```
[research-google t1]
ok browser restart
status: running
```

---

## 8. Browser Tab Management

### 8.1 `actionbook browser list-tabs --session <SID>`

> Addressing level: **Session**
> command: `browser list-tabs`

List all tabs in a specified session.

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `--session <SID>` | string | Yes | Session ID |

**JSON `data`:**

```json
{
  "total_tabs": 1,
  "tabs": [
    {
      "tab_id": "t1",
      "url": "https://google.com",
      "title": "Google",
      "native_tab_id": 391
    }
  ]
}
```

**Text output:**
```
[research-google]
1 tab
[t1] Google
https://google.com
```

---

### 8.2 `actionbook browser new-tab <url>... --session <SID>`

> Addressing level: **Session**
> command: `browser new-tab`
> alias: `browser open`

Open a new tab in the specified session.

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `<url>` | string[] | Yes | One or more URLs to open |
| `--session <SID>` | string | Yes | Session ID |
| `--new-window` | bool | No | Open in a new window |
| `--window <WID>` | string | No | Open in a specified window |
| `--tab <TID>` / `--set-tab-id <TID>` | string | No | Set a custom tab ID. When opening multiple URLs, repeat once per URL in order. |

**JSON `data`:**

```json
{
  "tab": {
    "tab_id": "t2",
    "url": "https://example.com",
    "title": "Example Domain",
    "native_tab_id": 392
  },
  "created": true,
  "new_window": false
}
```

**Text output:**
```
[research-google t2] https://example.com
ok browser new-tab
title: Example Domain
```

When multiple URLs are provided, the command opens them in order. If any URL
fails, the command exits non-zero and reports both the opened tabs and the
failed URLs.

---

### 8.3 `actionbook browser close-tab --session <SID> --tab <TID>`

> Addressing level: **Tab**
> command: `browser close-tab`

Close the specified tab.

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `--session <SID>` | string | Yes | Session ID |
| `--tab <TID>` | string | Yes | Tab ID |

**JSON `data`:**

```json
{
  "closed_tab_id": "t2"
}
```

**Text output:**
```
[research-google t2]
ok browser close-tab
```

---

## 9. Browser Navigation

All Navigation commands have addressing level: **Tab**, requiring `--session <SID> --tab <TID>`.

### 9.1 `actionbook browser goto <url>`

> command: `browser goto`

Navigate to the specified URL.

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `<url>` | string | Yes | Target URL |
| `--session <SID>` | string | Yes | Session ID |
| `--tab <TID>` | string | Yes | Tab ID |

**JSON `data`:**

```json
{
  "kind": "goto",
  "requested_url": "https://google.com/search?q=actionbook",
  "from_url": "https://google.com",
  "to_url": "https://google.com/search?q=actionbook",
  "title": "actionbook - Google Search"
}
```

**Text output:**
```
[research-google t1] https://google.com/search?q=actionbook
ok browser goto
title: actionbook - Google Search
```

---

### 9.2 `actionbook browser back`

> command: `browser back`

Navigate back.

**Parameters:** `--session <SID> --tab <TID>`

**JSON `data`:** Same structure as `goto`, with `kind` = `"back"`.

> Rule: After a successful goto/back/forward/reload, `context.url` must be updated to the post-navigation URL, and `context.title` is updated synchronously when known.

---

### 9.3 `actionbook browser forward`

> command: `browser forward`

Navigate forward.

**Parameters:** `--session <SID> --tab <TID>`

**JSON `data`:** Same structure as `goto`, with `kind` = `"forward"`.

---

### 9.4 `actionbook browser reload`

> command: `browser reload`

Reload the page.

**Parameters:** `--session <SID> --tab <TID>`

**JSON `data`:** Same structure as `goto`, with `kind` = `"reload"`.

---

## 10. Browser Observation

All Observation commands have addressing level: **Tab** (unless otherwise noted).

### 10.1 `actionbook browser snapshot`

> command: `browser snapshot`

Capture an accessibility tree snapshot of the page.

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `--session <SID>` | string | Yes | Session ID |
| `--tab <TID>` | string | Yes | Tab ID |
| `--interactive` | bool | No | Include only interactive elements |
| `--cursor` | bool | No (default: true) | Include mouse/focus-interactive custom elements (cursor:pointer, onclick, tabindex, etc.) — always enabled by default |
| `--compact` | bool | No | Compact output, remove empty structural nodes |
| `--depth <n>` | int | No | Limit maximum tree depth |
| `--selector <sel>` | string | No | Limit to a specific subtree |

**JSON `data`:**

```json
{
  "format": "snapshot",
  "path": "/Users/alice/.actionbook/sessions/s1/snapshot_1711900000000.txt",
  "nodes": [
    {
      "ref": "e1",
      "role": "textbox",
      "name": "Search",
      "value": ""
    }
  ],
  "stats": {
    "node_count": 2,
    "interactive_count": 2
  }
}
```

> Note: snapshot content is saved to a file at `data.path`. To read the content, use the file path directly. `data.nodes` provides the structured tree for programmatic use.

**Text output:**
```
[research-google t1] https://google.com
Refs are stable across snapshots — if the DOM node stays the same, the ref stays the same.
output saved to /Users/alice/.actionbook/sessions/s1/snapshot_1711900000000.txt
```

> When truncated, `meta.truncated = true`.

---

### 10.2 `actionbook browser screenshot <path>`

> command: `browser screenshot`

Capture a page screenshot.

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `<path>` | string | Yes | Output file path |
| `--session <SID>` | string | Yes | Session ID |
| `--tab <TID>` | string | Yes | Tab ID |
| `--full` | bool | No | Capture the full page (not just the current viewport) |
| `--annotate` | bool | No | Overlay numbered labels marking interactive elements, where `[N]` corresponds to ref `@eN` |
| `--screenshot-quality <0-100>` | int | No | JPEG quality (effective only for jpeg) |
| `--screenshot-format <png\|jpeg>` | string | No | Image format |
| `--selector <sel>` | string | No | Limit to a specific sub-region |

**JSON `data`:**

```json
{
  "artifact": {
    "path": "/tmp/google.png",
    "mime_type": "image/png",
    "bytes": 183920
  }
}
```

**Text output:**
```
[research-google t1] https://google.com
ok browser screenshot
path: /tmp/google.png
```

---

### 10.3 `actionbook browser pdf <path>`

> command: `browser pdf`

Save the current page as a PDF (similar to print export).

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `<path>` | string | Yes | Output file path |
| `--session <SID>` | string | Yes | Session ID |
| `--tab <TID>` | string | Yes | Tab ID |

**JSON `data`:** Same as `screenshot`, with `artifact.mime_type` = `"application/pdf"`.

**Text output:**
```
[research-google t1] https://google.com
ok browser pdf
path: /tmp/google.pdf
```

---

### 10.4 `actionbook browser title`

> command: `browser title`

Get the page title.

**Parameters:** `--session <SID> --tab <TID>`

**JSON `data`:** `{ "value": "Google" }`

**Text output:**
```
[research-google t1] https://google.com
Google
```

---

### 10.5 `actionbook browser url`

> command: `browser url`

Get the current page URL.

**Parameters:** `--session <SID> --tab <TID>`

**JSON `data`:** `{ "value": "https://google.com" }`

**Text output:**
```
[research-google t1] https://google.com
https://google.com
```

---

### 10.6 `actionbook browser viewport`

> command: `browser viewport`

Get the viewport dimensions.

**Parameters:** `--session <SID> --tab <TID>`

**JSON `data`:** `{ "width": 1440, "height": 900 }`

**Text output:**
```
[research-google t1] https://google.com
1440x900
```

---

### 10.7 `actionbook browser query <mode> <query_str>`

> command: `browser query`

Element query command with cardinality constraints.

**Subcommand forms:**

```
actionbook browser query one <query_str> --session <SID> --tab <TID>
actionbook browser query all <query_str> --session <SID> --tab <TID>
actionbook browser query nth <n> <query_str> --session <SID> --tab <TID>
actionbook browser query count <query_str> --session <SID> --tab <TID>
```

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `<mode>` | `one\|all\|nth\|count` | Yes | Query mode |
| `<query_str>` | string | Yes | CSS selector or extended syntax |
| `<n>` | int | Required for nth mode | 1-based index |
| `--session <SID>` | string | Yes | Session ID |
| `--tab <TID>` | string | Yes | Tab ID |

**Supported `query_str` syntax:**
- Standard CSS selectors (`.item`, `#some_id`, `.c > .a > input[name=b]`)
- Extended syntax (jQuery-inspired):
  - `:visible`
  - `:contains(...)`
  - `:has(...)`
  - `:enabled`
  - `:disabled`
  - `:checked`

**Matched element structure:**

```json
{
  "selector": ".item:nth-of-type(1)",
  "tag": "div",
  "text": "Item A",
  "visible": true,
  "enabled": true
}
```

#### mode = `one`

- Success condition: exactly 1 match
- 0 matches -> `ELEMENT_NOT_FOUND`
- More than 1 match -> `MULTIPLE_MATCHES`

**JSON `data` (success):**

```json
{
  "mode": "one",
  "query": ".item",
  "count": 1,
  "item": { "selector": ".item:nth-of-type(1)", "tag": "div", "text": "Item A", "visible": true, "enabled": true }
}
```

**JSON `error` (multiple matches):**

```json
{
  "code": "MULTIPLE_MATCHES",
  "message": "Query mode 'one' requires exactly 1 match, found 3",
  "retryable": false,
  "details": { "query": ".item", "count": 3, "sample_selectors": [".item:nth-of-type(1)", ".item:nth-of-type(2)", ".item:nth-of-type(3)"] }
}
```

**Text output (success):**
```
[research-google t1] https://example.com
1 match
selector: .item:nth-of-type(1)
text: Item A
```

#### mode = `all`

- Always returns a list
- 0 matches is still considered success, `items = []`

**JSON `data`:**

```json
{
  "mode": "all",
  "query": ".item",
  "count": 3,
  "items": [
    { "selector": ".item:nth-of-type(1)", "tag": "div", "text": "Item A", "visible": true, "enabled": true }
  ]
}
```

**Text output:**
```
[research-google t1] https://example.com
3 matches
1. .item:nth-of-type(1)
   Item A
```

#### mode = `nth <n>`

- `n` is 1-based
- `n > count` -> `INDEX_OUT_OF_RANGE`

**JSON `data`:**

```json
{
  "mode": "nth",
  "query": ".item",
  "index": 2,
  "count": 3,
  "item": { "selector": ".item:nth-of-type(2)", "tag": "div", "text": "Item B", "visible": true, "enabled": true }
}
```

**Text output:**
```
[research-google t1] https://example.com
match 2/3
selector: .item:nth-of-type(2)
text: Item B
```

#### mode = `count`

- Only returns the match count, does not return element details
- 0 matches is still considered success

**JSON `data`:**

```json
{
  "mode": "count",
  "query": ".item",
  "count": 3
}
```

**Text output:**
```
[research-google t1] https://example.com
3
```

---

### 10.8 Read Commands (html / text / value / attr / attrs / box / styles)

Unified addressing: **Tab**. Unified JSON structure.

#### `actionbook browser html <selector>`

> command: `browser html`

Get the outer HTML of an element.

**Parameters:** `<selector>` (required), `--session <SID> --tab <TID>`

**JSON `data`:**

```json
{
  "target": { "selector": "#title" },
  "value": "<h1 id=\"title\">Example Domain</h1>"
}
```

**Text output:**
```
[research-google t1] https://example.com
<h1 id="title">Example Domain</h1>
```

#### `actionbook browser text <selector>`

> command: `browser text`

Get the inner text of an element.

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `<selector>` | string | Yes | Target element |
| `--mode <raw\|readability>` | string | No | Text extraction mode |
| `--session <SID>` | string | Yes | Session ID |
| `--tab <TID>` | string | Yes | Tab ID |

**JSON `data`:** `{ "target": { "selector": "#title" }, "value": "Example Domain" }`

**Text output:**
```
[research-google t1] https://example.com
Example Domain
```

#### `actionbook browser value <selector>`

> command: `browser value`

Get the value of an input element.

**Parameters:** `<selector>` (required), `--session <SID> --tab <TID>`

**JSON `data`:** `{ "target": { "selector": "#email" }, "value": "user@example.com" }`

#### `actionbook browser attr <selector> <name>`

> command: `browser attr`

Get a specific attribute of an element.

**Parameters:** `<selector>` (required), `<name>` (required), `--session <SID> --tab <TID>`

**JSON `data`:** `{ "target": { "selector": "a.link" }, "value": "https://google.com" }`

**Text output:**
```
[research-google t1] https://example.com
https://google.com
```

#### `actionbook browser attrs <selector>`

> command: `browser attrs`

Get all attributes of an element.

**Parameters:** `<selector>` (required), `--session <SID> --tab <TID>`

**JSON `data`:** `{ "target": { "selector": "#btn" }, "value": { "id": "btn", "type": "submit", "class": "primary" } }`

**Text output:** Key-value list.

#### `actionbook browser box <selector>`

> command: `browser box`

Get the bounding box of an element.

**Parameters:** `<selector>` (required), `--session <SID> --tab <TID>`

**JSON `data`:** `{ "target": { "selector": "#btn" }, "value": { "x": 10, "y": 20, "width": 120, "height": 32 } }`

**Text output:**
```
[research-google t1] https://example.com
x: 10
y: 20
width: 120
height: 32
```

#### `actionbook browser styles <selector> [names...]`

> command: `browser styles`

Get the computed styles of an element.

**Parameters:** `<selector>` (required), `[names...]` (optional, specify property names), `--session <SID> --tab <TID>`

**JSON `data`:** `{ "target": { "selector": "#btn" }, "value": { "color": "rgb(0,0,0)", "font-size": "14px" } }`

**Text output:** Requested style names and values.

---

### 10.9 `actionbook browser describe <selector>`

> command: `browser describe`

Return a rule-based summary of an element (deterministically generated, no LLM invocation).

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `<selector>` | string | Yes | Target element |
| `--nearby` | bool | No | Additionally return one level of shallow nearby context |
| `--session <SID>` | string | Yes | Session ID |
| `--tab <TID>` | string | Yes | Tab ID |

**Summary generation rules:**
- Based on DOM tag, ARIA role, accessible name, label/text/placeholder/value, key attributes (type, href), current state (disabled, checked, selected)
- No additional semantic inference
- Assembly priority: role -> name -> qualifiers

**`--nearby` constraints:**
- Only returns 1 level, no recursion
- At most 1 parent
- At most 1 previous_sibling / next_sibling each
- At most 3 children
- Only returns nodes that have a name, text, or interactive significance

**JSON `data` (without --nearby):**

```json
{
  "target": { "selector": "button[type=submit]" },
  "summary": "button \"Google Search\"",
  "role": "button",
  "name": "Google Search",
  "tag": "button",
  "attributes": { "type": "submit" },
  "state": { "visible": true, "enabled": true },
  "nearby": null
}
```

**JSON `data` (with --nearby):**

```json
{
  "target": { "selector": "button[type=submit]" },
  "summary": "button \"Edit\"",
  "role": "button",
  "name": "Edit",
  "tag": "button",
  "attributes": { "type": "button" },
  "state": { "visible": true, "enabled": true },
  "nearby": {
    "parent": "listitem \"John Smith\"",
    "previous_sibling": "text \"John Smith\"",
    "next_sibling": null,
    "children": []
  }
}
```

**Text output:**
```
[research-google t1] https://google.com
button "Google Search"
```

**Text output (with --nearby):**
```
[research-google t1] https://google.com
button "Edit"
parent: listitem "John Smith"
previous_sibling: text "John Smith"
```

---

### 10.10 `actionbook browser state <selector>`

> command: `browser state`

Return the interactive state of an element.

**Parameters:** `<selector>` (required), `--session <SID> --tab <TID>`

**JSON `data`:**

```json
{
  "target": { "selector": "#search" },
  "state": {
    "visible": true,
    "enabled": true,
    "checked": false,
    "focused": true,
    "editable": true,
    "selected": false
  }
}
```

**Text output:**
```
[research-google t1] https://google.com
visible: true
enabled: true
checked: false
focused: true
editable: true
selected: false
```

---

### 10.11 `actionbook browser inspect-point <coordinates>`

> command: `browser inspect-point`

Inspect the element at specified coordinates (recommended for use with screenshot).

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `<coordinates>` | string | Yes | Format `x,y` |
| `--parent-depth <n>` | int | No | Number of parent levels to trace upward |
| `--session <SID>` | string | Yes | Session ID |
| `--tab <TID>` | string | Yes | Tab ID |

**JSON `data`:**

```json
{
  "point": { "x": 420, "y": 310 },
  "element": {
    "role": "button",
    "name": "Google Search",
    "selector": "input[name=btnK]"
  },
  "parents": [],
  "screenshot_path": null
}
```

**Text output:**
```
[research-google t1] https://google.com
button "Google Search"
selector: input[name=btnK]
point: 420,310
```

---

### 10.12 `actionbook browser logs console`

> command: `browser logs console`

Get console logs.

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `--session <SID>` | string | Yes | Session ID |
| `--tab <TID>` | string | Yes | Tab ID |
| `--level <level[,level...]>` | string | No | Filter by level (comma-separated for multiple values) |
| `--tail <n>` | int | No | Return only the last n entries |
| `--since <id>` | string | No | Return only logs after the specified ID |
| `--clear` | bool | No | Clear logs after retrieval |

**JSON `data`:**

```json
{
  "items": [
    {
      "id": "log-1",
      "level": "info",
      "text": "App mounted",
      "source": "app.js",
      "timestamp_ms": 1710000000000
    }
  ],
  "cleared": false
}
```

**Text output:**
```
[research-google t1] https://example.com
1 log
info 1710000000000 app.js App mounted
```

---

### 10.13 `actionbook browser logs errors`

> command: `browser logs errors`

Get error logs.

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `--session <SID>` | string | Yes | Session ID |
| `--tab <TID>` | string | Yes | Tab ID |
| `--source <file>` | string | No | Filter by error source file |
| `--tail <n>` | int | No | Return only the last n entries |
| `--since <id>` | string | No | Return only logs after the specified ID |
| `--clear` | bool | No | Clear logs after retrieval |

**JSON `data`:** Same structure as `logs console`.

---

## 11. Browser Interaction

All Interaction commands have addressing level: **Tab**.

### 11.1 Action Commands (click / hover / focus / press / drag / mouse-move / scroll)

Unified JSON `data` structure:

```json
{
  "action": "click",
  "target": { "selector": "button[type=submit]" },
  "changed": {
    "url_changed": false,
    "focus_changed": true
  }
}
```

**Text output:**
```
[research-google t1] https://google.com
ok browser click
target: button[type=submit]
```

#### `actionbook browser click <selector|coordinates>`

> command: `browser click`

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `<selector\|coordinates>` | string | Yes | Target element or coordinates |
| `--session <SID>` | string | Yes | Session ID |
| `--tab <TID>` | string | Yes | Tab ID |
| `--new-tab` | bool | No | If the target has an href, open in a new tab |
| `--button <left\|right\|middle>` | string | No | Mouse button, default left |
| `--count <n>` | int | No | Click count (2 = double-click) |

> Special rule: If the click causes a navigation, `context.url` must be updated to the post-navigation URL.

#### `actionbook browser hover <selector>`

> command: `browser hover`

**Parameters:** `<selector>` (required), `--session <SID> --tab <TID>`

#### `actionbook browser focus <selector>`

> command: `browser focus`

**Parameters:** `<selector>` (required), `--session <SID> --tab <TID>`

#### `actionbook browser press <key-or-chord>`

> command: `browser press`

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `<key-or-chord>` | string | Yes | Single key or key combination (e.g., `Enter`, `Control+A`, `Shift+Tab`) |
| `--session <SID>` | string | Yes | Session ID |
| `--tab <TID>` | string | Yes | Tab ID |

> Special rule: For chords, the target may be omitted and replaced with `keys`.

#### `actionbook browser drag <selector> <selector|coordinates>`

> command: `browser drag`

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `<selector>` | string | Yes | Drag source element |
| `<selector\|coordinates>` | string | Yes | Drop target element or coordinates |
| `--session <SID>` | string | Yes | Session ID |
| `--tab <TID>` | string | Yes | Tab ID |
| `--button <left\|right\|middle>` | string | No | Mouse button |

#### `actionbook browser mouse-move <coordinates>`

> command: `browser mouse-move`

Move the mouse to absolute coordinates.

**Parameters:** `<coordinates>` (required, format `x,y`), `--session <SID> --tab <TID>`

#### `actionbook browser cursor-position`

> command: `browser cursor-position`

Get the current mouse position.

**Parameters:** `--session <SID> --tab <TID>`

**JSON `data`:** `{ "x": 420, "y": 310 }`

#### `actionbook browser scroll`

> command: `browser scroll`

Scroll the page or a container.

**Three subcommand forms:**

```
actionbook browser scroll up|down|left|right <pixels> --session <SID> --tab <TID> [--container <selector>]
actionbook browser scroll top|bottom --session <SID> --tab <TID> [--container <selector>]
actionbook browser scroll into-view <selector> --session <SID> --tab <TID> [--align <start|center|end|nearest>]
```

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| Direction/action | `up\|down\|left\|right\|top\|bottom\|into-view` | Yes | Scroll method |
| `<pixels>` | int | Required for directional scrolling | Scroll distance in pixels |
| `<selector>` | string | Required for `into-view` | Target element |
| `--container <selector>` | string | No | Scroll within a specified container |
| `--align <start\|center\|end\|nearest>` | string | No | Alignment for `into-view` |

> Special rule: `data.changed.scroll_changed = true` indicates that scrolling displacement occurred.

---

### 11.2 Input Commands (type / fill / select / upload)

Unified JSON `data` structure:

```json
{
  "action": "fill",
  "target": { "selector": "textarea[name=q]" },
  "value_summary": { "text_length": 10 }
}
```

**Text output:**
```
[research-google t1] https://google.com
ok browser fill
target: textarea[name=q]
text_length: 10
```

#### `actionbook browser type <selector> <text>`

> command: `browser type`

Type text character by character (triggers keyboard events).

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `<selector>` | string | Yes | Target element |
| `<text>` | string | Yes | Text to type |
| `--session <SID>` | string | Yes | Session ID |
| `--tab <TID>` | string | Yes | Tab ID |

#### `actionbook browser fill <selector> <text>`

> command: `browser fill`

Directly set the value of an input field (triggers input event).

**Parameters:** Same as `type`.

#### `actionbook browser select <selector> <value>`

> command: `browser select`

Select a value from a dropdown list.

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `<selector>` | string | Yes | `<select>` element |
| `<value>` | string | Yes | Value to select |
| `--by-text` | bool | No | Match by display text instead of value attribute |
| `--session <SID>` | string | Yes | Session ID |
| `--tab <TID>` | string | Yes | Tab ID |

> `value_summary` = `{ "value": "...", "by_text": true|false }`

#### `actionbook browser upload <selector> <file...>`

> command: `browser upload`

Upload files to a file input.

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `<selector>` | string | Yes | File input element |
| `<file...>` | string[] | Yes | Absolute file paths (supports multiple) |
| `--session <SID>` | string | Yes | Session ID |
| `--tab <TID>` | string | Yes | Tab ID |

> `value_summary` = `{ "files": ["/abs/path/a.pdf"], "count": 1 }`

---

### 11.3 `actionbook browser eval <code>`

> command: `browser eval`

Execute JavaScript in the page context.

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `<code>` | string | Yes | JavaScript expression |
| `--session <SID>` | string | Yes | Session ID |
| `--tab <TID>` | string | Yes | Tab ID |

**JSON `data`:**

```json
{
  "value": 42,
  "type": "number",
  "preview": "42"
}
```

**Text output:**
```
[research-google t1] https://example.com
42
```

---

## 12. Browser Waiting

All Waiting commands have addressing level: **Tab**.

Unified JSON `data` structure:

```json
{
  "kind": "element",
  "satisfied": true,
  "elapsed_ms": 182,
  "observed_value": { "selector": "#loaded" }
}
```

**Text output:**
```
[research-google t1] https://example.com
ok browser wait element
elapsed_ms: 182
```

### 12.1 `actionbook browser wait element <selector>`

> command: `browser wait element`

Wait for an element to appear in the DOM.

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `<selector>` | string | Yes | Element selector to wait for |
| `--session <SID>` | string | Yes | Session ID |
| `--tab <TID>` | string | Yes | Tab ID |
| `--timeout <ms>` | u64 | Yes | Timeout in milliseconds |

**`data.kind`** = `"element"`
**`data.observed_value`** = `{ "selector": "#loaded" }`

---

### 12.2 `actionbook browser wait navigation`

> command: `browser wait navigation`

Wait for navigation to complete.

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `--session <SID>` | string | Yes | Session ID |
| `--tab <TID>` | string | Yes | Tab ID |
| `--timeout <ms>` | u64 | Yes | Timeout in milliseconds |

**`data.kind`** = `"navigation"`

---

### 12.3 `actionbook browser wait network-idle`

> command: `browser wait network-idle`

Wait for network to become idle.

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `--session <SID>` | string | Yes | Session ID |
| `--tab <TID>` | string | Yes | Tab ID |
| `--timeout <ms>` | u64 | Yes | Timeout in milliseconds |

**`data.kind`** = `"network-idle"`

---

### 12.4 `actionbook browser wait condition <expression>`

> command: `browser wait condition`

Wait for a JS expression to become truthy.

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `<expression>` | string | Yes | JavaScript expression (should return a truthy value) |
| `--session <SID>` | string | Yes | Session ID |
| `--tab <TID>` | string | Yes | Tab ID |
| `--timeout <ms>` | u64 | Yes | Timeout in milliseconds |

**`data.kind`** = `"condition"`

---

## 13. Browser Cookies

All Cookies commands have addressing level: **Session** (`--session` only, no `--tab` required).

### 13.1 `actionbook browser cookies list`

> command: `browser cookies list`

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `--session <SID>` | string | Yes | Session ID |
| `--domain <domain>` | string | No | Filter by domain |

**JSON `data`:**

```json
{
  "items": [
    {
      "name": "SID",
      "value": "xxx",
      "domain": ".google.com",
      "path": "/",
      "http_only": true,
      "secure": true,
      "same_site": "Lax",
      "expires": null
    }
  ]
}
```

**Text output:**
```
[research-google]
1 cookie
SID .google.com /
```

---

### 13.2 `actionbook browser cookies get <name>`

> command: `browser cookies get`

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `<name>` | string | Yes | Cookie name |
| `--session <SID>` | string | Yes | Session ID |

**JSON `data`:** `{ "item": { ...cookie object } }`

---

### 13.3 `actionbook browser cookies set <name> <value>`

> command: `browser cookies set`

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `<name>` | string | Yes | Cookie name |
| `<value>` | string | Yes | Cookie value |
| `--session <SID>` | string | Yes | Session ID |
| `--domain` | string | No | Domain |
| `--path` | string | No | Path |
| `--secure` | bool | No | Secure flag |
| `--http-only` | bool | No | HttpOnly flag |
| `--same-site <Strict\|Lax\|None>` | string | No | SameSite policy |
| `--expires <timestamp>` | f64 | No | Expiration time (Unix timestamp) |

**JSON `data`:** `{ "action": "set", "affected": 1, "domain": ".google.com" }`

---

### 13.4 `actionbook browser cookies delete <name>`

> command: `browser cookies delete`

**Parameters:** `<name>` (required), `--session <SID>`

**JSON `data`:** `{ "action": "delete", "affected": 1 }`

---

### 13.5 `actionbook browser cookies clear`

> command: `browser cookies clear`

**Parameters:**

| Parameter | Type | Required | Description |
|------|------|------|------|
| `--session <SID>` | string | Yes | Session ID |
| `--domain <domain>` | string | No | Filter by domain |

**JSON `data`:** `{ "action": "clear", "affected": 5, "domain": ".google.com" }`

---

## 14. Browser Storage

All Storage commands have addressing level: **Tab** (`--session <SID> --tab <TID>`).

Two storage types share the same subcommand structure:
- `session-storage` -> `window.sessionStorage`
- `local-storage` -> `window.localStorage`

### 14.1 `actionbook browser session-storage|local-storage list`

> command: `browser local-storage list` / `browser session-storage list`

**Parameters:** `--session <SID> --tab <TID>`

**JSON `data`:**

```json
{
  "storage": "local",
  "items": [
    { "key": "theme", "value": "dark" }
  ]
}
```

**Text output:**
```
[research-google t1] https://example.com
1 key
theme=dark
```

---

### 14.2 `actionbook browser session-storage|local-storage get <key>`

> command: `browser local-storage get` / `browser session-storage get`

**Parameters:** `<key>` (required), `--session <SID> --tab <TID>`

**JSON `data`:** `{ "storage": "local", "item": { "key": "theme", "value": "dark" } }`

---

### 14.3 `actionbook browser session-storage|local-storage set <key> <value>`

> command: `browser local-storage set` / `browser session-storage set`

**Parameters:** `<key>` (required), `<value>` (required), `--session <SID> --tab <TID>`

**JSON `data`:** `{ "storage": "local", "action": "set", "affected": 1 }`

---

### 14.4 `actionbook browser session-storage|local-storage delete <key>`

> command: `browser local-storage delete` / `browser session-storage delete`

**Parameters:** `<key>` (required), `--session <SID> --tab <TID>`

**JSON `data`:** `{ "storage": "local", "action": "delete", "affected": 1 }`

---

### 14.5 `actionbook browser session-storage|local-storage clear <key>`

> command: `browser local-storage clear` / `browser session-storage clear`

Clear the stored value for the specified key.

**Parameters:** `<key>` (required), `--session <SID> --tab <TID>`

**JSON `data`:** `{ "storage": "local", "action": "clear", "affected": 1 }`

---

## Appendix: Command Overview (70 interfaces total)

### Non-Browser Commands (5)

| # | Command | Addressing Level | command name |
|---|------|----------|-----------|
| 1 | `search <query>` | — | `search` |
| 2 | `get <area_id>` | — | `get` |
| 3 | `setup` | — | `setup` |
| 4 | `help` | — | `help` |
| 5 | `--version` | — | `version` |

### Browser Lifecycle (5)

| # | Command | Addressing Level | command name |
|---|------|----------|-----------|
| 6 | `browser start` | Global | `browser start` |
| 7 | `browser list-sessions` | Global | `browser list-sessions` |
| 8 | `browser status` | Session | `browser status` |
| 9 | `browser close` | Session | `browser close` |
| 10 | `browser restart` | Session | `browser restart` |

### Browser Tab Management (3)

| # | Command | Addressing Level | command name |
|---|------|----------|-----------|
| 11 | `browser list-tabs` | Session | `browser list-tabs` |
| 12 | `browser new-tab` / `open` | Session | `browser new-tab` |
| 13 | `browser close-tab` | Tab | `browser close-tab` |

### Browser Navigation (4)

| # | Command | Addressing Level | command name |
|---|------|----------|-----------|
| 14 | `browser goto` | Tab | `browser goto` |
| 15 | `browser back` | Tab | `browser back` |
| 16 | `browser forward` | Tab | `browser forward` |
| 17 | `browser reload` | Tab | `browser reload` |

### Browser Observation (17)

| # | Command | Addressing Level | command name |
|---|------|----------|-----------|
| 18 | `browser snapshot` | Tab | `browser snapshot` |
| 19 | `browser screenshot` | Tab | `browser screenshot` |
| 20 | `browser pdf` | Tab | `browser pdf` |
| 21 | `browser title` | Tab | `browser title` |
| 22 | `browser url` | Tab | `browser url` |
| 23 | `browser viewport` | Tab | `browser viewport` |
| 24 | `browser query` | Tab | `browser query` |
| 25 | `browser html` | Tab | `browser html` |
| 26 | `browser text` | Tab | `browser text` |
| 27 | `browser value` | Tab | `browser value` |
| 28 | `browser attr` | Tab | `browser attr` |
| 29 | `browser attrs` | Tab | `browser attrs` |
| 30 | `browser box` | Tab | `browser box` |
| 31 | `browser styles` | Tab | `browser styles` |
| 32 | `browser describe` | Tab | `browser describe` |
| 33 | `browser state` | Tab | `browser state` |
| 34 | `browser inspect-point` | Tab | `browser inspect-point` |

### Browser Logging (2)

| # | Command | Addressing Level | command name |
|---|------|----------|-----------|
| 35 | `browser logs console` | Tab | `browser logs console` |
| 36 | `browser logs errors` | Tab | `browser logs errors` |

### Browser Interaction (15)

| # | Command | Addressing Level | command name |
|---|------|----------|-----------|
| 37 | `browser click` | Tab | `browser click` |
| 38 | `browser type` | Tab | `browser type` |
| 39 | `browser fill` | Tab | `browser fill` |
| 40 | `browser select` | Tab | `browser select` |
| 41 | `browser hover` | Tab | `browser hover` |
| 42 | `browser focus` | Tab | `browser focus` |
| 43 | `browser press` | Tab | `browser press` |
| 44 | `browser drag` | Tab | `browser drag` |
| 45 | `browser upload` | Tab | `browser upload` |
| 46 | `browser eval` | Tab | `browser eval` |
| 47 | `browser mouse-move` | Tab | `browser mouse-move` |
| 48 | `browser cursor-position` | Tab | `browser cursor-position` |
| 49 | `browser scroll (direction)` | Tab | `browser scroll` |
| 50 | `browser scroll (top/bottom)` | Tab | `browser scroll` |
| 51 | `browser scroll into-view` | Tab | `browser scroll` |

### Browser Waiting (4)

| # | Command | Addressing Level | command name |
|---|------|----------|-----------|
| 52 | `browser wait element` | Tab | `browser wait element` |
| 53 | `browser wait navigation` | Tab | `browser wait navigation` |
| 54 | `browser wait network-idle` | Tab | `browser wait network-idle` |
| 55 | `browser wait condition` | Tab | `browser wait condition` |

### Browser Cookies (5)

| # | Command | Addressing Level | command name |
|---|------|----------|-----------|
| 56 | `browser cookies list` | Session | `browser cookies list` |
| 57 | `browser cookies get` | Session | `browser cookies get` |
| 58 | `browser cookies set` | Session | `browser cookies set` |
| 59 | `browser cookies delete` | Session | `browser cookies delete` |
| 60 | `browser cookies clear` | Session | `browser cookies clear` |

### Browser Storage (10)

| # | Command | Addressing Level | command name |
|---|------|----------|-----------|
| 61 | `browser session-storage list` | Tab | `browser session-storage list` |
| 62 | `browser session-storage get` | Tab | `browser session-storage get` |
| 63 | `browser session-storage set` | Tab | `browser session-storage set` |
| 64 | `browser session-storage delete` | Tab | `browser session-storage delete` |
| 65 | `browser session-storage clear` | Tab | `browser session-storage clear` |
| 66 | `browser local-storage list` | Tab | `browser local-storage list` |
| 67 | `browser local-storage get` | Tab | `browser local-storage get` |
| 68 | `browser local-storage set` | Tab | `browser local-storage set` |
| 69 | `browser local-storage delete` | Tab | `browser local-storage delete` |
| 70 | `browser local-storage clear` | Tab | `browser local-storage clear` |

**Total: 70 interfaces**

---

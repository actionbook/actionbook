# Actionbook for Dify

Actionbook is a browser action engine for AI agents. This Dify plugin brings Actionbook's up-to-date action manuals, verified selectors, and cloud browser automation into Dify workflows so agents can operate websites with less guessing and more resilience.

## What This Plugin Provides

| Tool | Description | Best For |
|------|-------------|----------|
| `search_actions` | Search Actionbook action manuals by keyword and optional domain | Finding relevant website actions and UI areas |
| `get_action_by_area_id` | Retrieve full action details, including verified selectors and supported interaction methods | Getting precise page structure before automation |
| `browser_create_session` | Start a cloud browser session via Hyperbrowser | Beginning a live browser workflow |
| `browser_operator` | Navigate, click, fill, type, wait, snapshot, and extract content | Executing browser steps on a live page |
| `browser_stop_session` | Stop the browser session and release resources | Cleaning up at the end of automation |

## Why Use Actionbook in Dify

- Bring structured website knowledge into your workflow with Actionbook action manuals instead of relying only on raw HTML or guessed selectors.
- Combine verified selectors with live browser automation for more reliable multi-step tasks.
- Help agents browse faster by starting from pre-indexed actions instead of exploring the DOM from scratch.
- Use accessibility snapshots as a fallback when a page has changed or a selector no longer works.

## Credentials

### Actionbook API Key (Optional)

The Actionbook API key is used for `search_actions` and `get_action_by_area_id`. You can leave it empty and use the free tier with basic limits, or add a key for higher quotas.

- Get a key from [actionbook.dev](https://actionbook.dev/?utm_source=dify)
- Manage keys at [Dashboard > API Keys](https://actionbook.dev/dashboard/api-keys?utm_source=dify)

### Hyperbrowser API Key (Required for Browser Tools)

The Hyperbrowser API key is required for live browser automation:

- `browser_create_session`
- `browser_operator`
- `browser_stop_session`

Get your key from [Hyperbrowser](https://app.hyperbrowser.ai/?utm_source=dify).

## Recommended Workflow

Typical best-practice workflow:

1. `search_actions("github login", domain="github.com")`
2. `get_action_by_area_id("<area_id from search_actions>")`
3. `browser_create_session()`
4. `browser_operator(session_id=..., cdp_url=<ws_endpoint from browser_create_session>, action="navigate", url="https://github.com/login")`
5. `browser_operator(session_id=..., cdp_url=..., action="fill", selector="<verified selector from get_action_by_area_id>", text="user@example.com")`
6. `browser_operator(session_id=..., cdp_url=..., action="click", selector="<verified selector from get_action_by_area_id>")`
7. `browser_operator(session_id=..., cdp_url=..., action="wait_navigation")` or `browser_operator(session_id=..., cdp_url=..., action="get_text")`
8. `browser_stop_session(session_id=...)`

How this maps to the actual tool behavior:

- `search_actions` and `get_action_by_area_id` are the recommended way to get verified selectors first, but they are not a hard prerequisite for `browser_operator`.
- `browser_operator` accepts either `session_id` or `cdp_url`; for multi-step workflows, pass `session_id` and preferably both for session recovery.
- `browser_create_session` returns `ws_endpoint`, which should be passed to `browser_operator` as `cdp_url`.
- `snapshot` is a fallback step when selectors fail or page state changes, not a mandatory success-path step.
- `browser_stop_session` should be called after `browser_create_session` workflows to release the remote session and persist profile state when applicable.

## Example Use Cases

- Log into websites and submit forms inside a Dify workflow
- Search or filter content on dynamic web apps
- Combine Actionbook search/get tools with browser automation for agentic task execution
- Recover from selector drift by taking a fresh `snapshot` and continuing the workflow

## Open Source & Community

- **GitHub**: [github.com/actionbook/actionbook](https://github.com/actionbook/actionbook?utm_source=dify)
- **Docs**: [actionbook.dev/docs](https://actionbook.dev/docs?utm_source=dify)
- **Discord**: [Join Discord](https://actionbook.dev/discord?utm_source=dify)
- **X / Twitter**: [@ActionbookHQ](https://x.com/ActionbookHQ)
- **Website**: [actionbook.dev](https://actionbook.dev/?utm_source=dify)

## Request Coverage

Suggest websites you want Actionbook to index at [actionbook.dev/request-website](https://actionbook.dev/request-website?utm_source=dify).

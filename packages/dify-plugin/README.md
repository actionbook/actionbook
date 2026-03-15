# Actionbook

**Browser Action Engine for AI Agents** — provides up-to-date action manuals and DOM structure, so your agent operates any website instantly without guessing.

## Tools

| Tool | Description | Best For |
|------|-------------|----------|
| `search_actions` | Search verified selectors by keyword/domain | Finding elements on any website |
| `get_action_by_area_id` | Get full selector details (CSS, XPath, Aria) | Precise element targeting |
| `browser_create_session` | Start a cloud browser session via Hyperbrowser | Headless browser workflows |
| `browser_operator` | Navigate, click, fill, snapshot, and more | Page interaction steps |
| `browser_stop_session` | Stop session and release resources | Cleanup after automation |

## Quick Start

### 1. Actionbook API Key (Optional)

Actionbook works **without an API key** with basic rate limits. For higher quotas, sign up at [actionbook.dev](https://actionbook.dev/?utm_source=dify) and get your key from [Dashboard > API Keys](https://actionbook.dev/dashboard/api-keys?utm_source=dify).

### 2. Get Your Hyperbrowser API Key

Browser tools (`browser_create_session`, `browser_operator`, `browser_stop_session`) require a [Hyperbrowser](https://www.hyperbrowser.ai/?utm_source=dify) API key. Sign up and get your key from the Hyperbrowser dashboard.

### 3. Add Tools to Your Workflow

Add Actionbook tools to any Dify workflow or agent. All selectors are community-verified and health-scored.

## Workflow Example

A typical end-to-end browser automation flow:

```
1. search_actions("login form", domain="github.com")
2. browser_create_session()  → returns session_id + ws_endpoint
3. browser_operator(session_id=…, cdp_url=…, action="navigate", url="https://github.com/login")
4. browser_operator(session_id=…, cdp_url=…, action="fill", selector="#login_field", text="user@email.com")
5. browser_operator(session_id=…, cdp_url=…, action="click", selector="input[type=submit]")
6. browser_operator(session_id=…, cdp_url=…, action="snapshot")  → inspect result
7. browser_stop_session(session_id=…)
```

## Open Source & Community

- **GitHub**: Star the repo, report issues, or contribute: [github.com/actionbook/actionbook](https://github.com/actionbook/actionbook?utm_source=dify)
- **Discord**: Questions, feedback, or just say hi — DM us anytime: [Join Discord](https://actionbook.dev/discord?utm_source=dify)
- **Twitter/X**: Follow for updates and announcements: [@ActionbookHQ](https://x.com/ActionbookHQ)
- **Website**: [actionbook.dev](https://actionbook.dev/?utm_source=dify)

---

**Request a Website** — Suggest websites you want Actionbook to index: [actionbook.dev/request-website](https://actionbook.dev/request-website?utm_source=dify)

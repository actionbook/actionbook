![Actionbook Cover](https://github.com/user-attachments/assets/18f55ca3-2c25-4f6a-a518-1b07baf8b4dd)

<div align="center">

### Actionbook

![GitHub last commit](https://img.shields.io/github/last-commit/actionbook/actionbook) [![NPM Downloads](https://img.shields.io/npm/d18m/%40actionbookdev%2Fcli)](https://www.npmjs.com/package/@actionbookdev/cli) [![npm version](https://img.shields.io/npm/v/%40actionbookdev%2Fcli)](https://www.npmjs.com/package/@actionbookdev/cli) [![skills](https://img.shields.io/badge/skills-ready-blue)](https://skills.sh/actionbook/actionbook/actionbook)

**Parallel Action CLI for AI agents.**
<br />
Run 50 actions across 20 tabs at once.

[Website](https://actionbook.dev) · [GitHub](https://github.com/actionbook/actionbook) · [X](https://x.com/ActionbookHQ) · [Discord](https://actionbook.dev/discord)

</div>

<br />

## Table of Contents

- [Why Actionbook?](#why-actionbook)
- [Installation](#installation)
- [Quick Start](#quick-start)
- [AI Agent Skills](#ai-agent-skills)
- [Examples](#examples)
- [Available Tools](#available-tools)
- [Documentation](#documentation)
- [Contributing](#contributing)

## Why Actionbook?

### ❌ Without Actionbook

- **Slow.** Agents take a snapshot after every single step, parse the page, then decide what to do next. Searching one room on Airbnb takes 15 minutes.
- **Brittle.** Modern websites use virtual DOMs, streaming components, and SPAs. Agents don't understand these rendering mechanisms, so they fail to interact with dropdowns, date pickers, and dynamic content.
- **One at a time.** Your agent finishes one page before it can start the next. Need to check 30 company websites? That's 30 rounds, one after another.

### ✅ With Actionbook

- **10x faster.** Action manuals tell agents exactly what to do. No parsing, no guessing.
- **Accurate.** Built for virtual DOMs, SPAs, and streaming components. Agents operate reliably.
- **Concurrent.** Stateless architecture. Operate dozens of tabs in parallel.

See an agent visits **192** First Round portfolio company websites and collects their taglines in 2 minutes. (**Video is not sped up**)

https://github.com/user-attachments/assets/35079a19-7236-47a8-87ed-3edf6436c2bf

## Installation

Install via npm:

```bash
npm install -g @actionbookdev/cli
```

Or build from source:

```bash
cargo install --git https://github.com/actionbook/actionbook --path packages/cli --locked
```

The Rust-based CLI uses your existing system browser (Chrome, Brave, Edge, Arc, Chromium), so no extra browser install step is required.

## Quick Start

```bash
actionbook browser start

# Open tabs
actionbook browser open https://stripe.com --session s1
actionbook browser open https://linear.app --session s1
actionbook browser open https://vercel.com --session s1

# Operate all tabs concurrently
actionbook browser snapshot --session s1 --tab t1 &
actionbook browser snapshot --session s1 --tab t2 &
actionbook browser snapshot --session s1 --tab t3 &

# Interact with each tab using refs from its snapshot
actionbook browser click @e5 --session s1 --tab t1
actionbook browser fill @e3 "hello" --session s1 --tab t2
actionbook browser click @e8 --session s1 --tab t3
```

When working with any AI coding assistant (Claude Code, Cursor, etc.), add this to your prompt:

```
Use Actionbook to understand and operate the web page.
```

The agent will automatically use the CLI to fetch action manuals and execute browser operations.

## AI Agent Skills

Actionbook ships with Agent Skills that teach your AI agent how to use the CLI.

**Claude Code, Cursor, Codex, Windsurf, Antigravity, Opencode** — one command:

```bash
npx skills add actionbook/actionbook
```

**Hermes** — one command (registers the skill in `~/.hermes/skills/`):

```bash
hermes skills install actionbook -y
```

Then start a chat and say things like *"use actionbook to open google.com and search for anthropic"*. Hermes auto-activates the skill and drives the CLI for you.

## Examples

Explore real-world examples in the [Examples Documentation](https://actionbook.dev/docs/examples).


## Available Tools

Actionbook provides tools for searching and retrieving action manuals. See the [CLI Reference](https://actionbook.dev/docs/api-reference/cli) for the full command list. If you're using the MCP integration, see the [MCP Tools Reference](https://actionbook.dev/docs/api-reference/mcp-tools).


## Documentation

For comprehensive guides, API references, and tutorials, visit our documentation site:

**[actionbook.dev/docs](https://actionbook.dev/docs)**

## Stay tuned

We move fast. Star Actionbook on Github to support and get latest information.

![Star Actionbook](https://github.com/user-attachments/assets/2d6571cb-4e12-438b-b7bf-9a4b68ef2be3)

Join the community:

- [Chat with us on Discord](https://actionbook.dev/discord) - Get help, share your agents, and discuss ideas
- [Follow @ActionbookHQ on X](https://x.com/ActionbookHQ) - Product updates and announcements

## Contributing

- **[Read the Contributing Guide](CONTRIBUTING.md)** - See repository setup, package layout, and validation workflows for the public repo.
- **[Request a Website](https://actionbook.dev/request-website)** - Suggest websites you want Actionbook to index.

## License

See [LICENSE](LICENSE) for the license details.

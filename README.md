![Actionbook Cover](https://github.com/user-attachments/assets/18f55ca3-2c25-4f6a-a518-1b07baf8b4dd)

<div align="center">

### Actionbook

![GitHub last commit](https://img.shields.io/github/last-commit/actionbook/actionbook) [![NPM Downloads](https://img.shields.io/npm/d18m/%40actionbookdev%2Fcli)](https://www.npmjs.com/package/@actionbookdev/cli) [![npm version](https://img.shields.io/npm/v/%40actionbookdev%2Fcli)](https://www.npmjs.com/package/@actionbookdev/cli) [![skills](https://img.shields.io/badge/skills-ready-blue)](https://skills.sh/actionbook/actionbook/actionbook)

**Browser Action Engine for AI Agents**
<br />
Actionbook provides up-to-date action manuals built for the modern web,
<br />
so your agent operates any website instantly. One tab or dozens, concurrently.

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

See an agent visits **192** First Round portfolio company websites and collects their taglines in 3 minutes. (**Video is not sped up or edited**)

https://github.com/user-attachments/assets/174458b8-efd3-4fa6-9ef4-2422587edf4b

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

When working with any AI coding assistant (Claude Code, Cursor, etc.), add this to your prompt:

```
Use Actionbook to understand and operate the web page.
```

The agent will automatically use the CLI to fetch action manuals and execute browser operations.

## AI Agent Skills

Actionbook ships with Agent Skills that teach your AI agent how to use the CLI. Add them with one command:

```bash
npx skills add actionbook/actionbook
```


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

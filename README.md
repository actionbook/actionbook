![Actionbook Cover](https://github.com/user-attachments/assets/18f55ca3-2c25-4f6a-a518-1b07baf8b4dd)

<div align="center">

### Actionbook

![GitHub last commit](https://img.shields.io/github/last-commit/actionbook/actionbook) [![NPM Downloads](https://img.shields.io/npm/d18m/%40actionbookdev%2Fcli)](https://www.npmjs.com/package/@actionbookdev/cli) [![npm version](https://img.shields.io/npm/v/%40actionbookdev%2Fcli)](https://www.npmjs.com/package/@actionbookdev/cli) [![skills](https://img.shields.io/badge/skills-ready-blue)](https://skills.sh/actionbook/actionbook/actionbook)




**Browser Action Engine for AI Agents**
<br />
Actionbook provides up-to-date action manuals and DOM structure,
<br />
so your agent operates any website instantly without guessing.

[Website](https://actionbook.dev) · [GitHub](https://github.com/actionbook/actionbook) · [X](https://x.com/ActionbookHQ) · [Discord](https://discord.gg/7sKKp7XQ2d)

</div>

<br />

## Table of Contents

- [Why Actionbook?](#why-actionbook)
- [Quick Start](#quick-start)
- [Installation](#installation)
- [Examples](#examples)
- [Available Tools](#available-tools)
- [Documentation](#documentation)
- [Development](#development)
- [Contributing](#contributing)

## Why Actionbook?

### ❌ Without Actionbook

Building reliable browser agents is difficult and expensive:

- **Slow Execution:** Agents waste time parsing full HTML pages to find elements.
- **High Token Costs:** Sending entire DOM trees to LLMs consumes massive context windows.
- **Brittle Selectors:** Updates to website UIs break hardcoded selectors and agent logic immediately.
- **Hallucinations:** LLMs often guess incorrect actions when faced with complex, unstructured DOMs.

### ✅ With Actionbook

Actionbook places up-to-date action manuals with the relevant DOM selectors directly into your LLM's context.

- **10x Faster:** Agents access pre-computed "Action manuals" to know exactly what to do without exploring.
- **100x Token Savings:** Instead of whole HTML page, agents receive only related DOM elements in concise, semantic JSON definitions.
- **Resilient Automation:** Action manuals are maintained and versioned. If a site changes, the manual is updated, not your agent.
- **Universal Compatibility:** Works with any LLM (OpenAI, Anthropic, Gemini) and any AI operator framework.

See how Actionbook enables an agent to complete an Airbnb search task 10x faster.

https://github.com/user-attachments/assets/9f896fe7-296a-44b3-8592-931a099612de

## Quick Start

Get started with Actionbook in under 2 minutes:

**Step 1: Install the CLI**

```bash
npm install -g @actionbookdev/cli
```

The Rust-based CLI uses your existing system browser (Chrome, Brave, Edge, Arc, Chromium), so no extra browser install step is required.

**Step 2: Use with any AI Agent**

When working with any AI coding assistant (Claude Code, Cursor, etc.), add this to your prompt:

```
Use Actionbook to understand and operate the web page.
```

The agent will automatically use the CLI to fetch action manuals and execute browser operations.

**Step 3 (Optional): Add the Skill**

For enhanced agent integration, add the Actionbook skill:

```bash
npx skills add actionbook/actionbook
```

## Installation

Actionbook provides three integration methods:

- **CLI (Recommended)**: Best for AI agents and general automation.
- **MCP Server**: For AI IDEs like Cursor and Claude.
- **JavaScript SDK**: For custom programmatic integration.

For detailed installation instructions, please visit the [Installation Guide](https://actionbook.dev/docs/guides/installation).


## Examples

Explore real-world examples in the [Examples Documentation](https://actionbook.dev/docs/examples).


## Available Tools

Actionbook provides tools for searching and retrieving action manuals.

Check out the [CLI Reference](https://actionbook.dev/docs/api-reference/cli) and [MCP Tools Reference](https://actionbook.dev/docs/api-reference/mcp-tools).


## Documentation

For comprehensive guides, API references, and tutorials, visit our documentation site:

**[actionbook.dev/docs](https://actionbook.dev/docs)**

## Stay tuned

We move fast. Star Actionbook on Github to support and get latest information.

![Star Actionbook](https://github.com/user-attachments/assets/2d6571cb-4e12-438b-b7bf-9a4b68ef2be3)

Join the community:

- [Chat with us on Discord](https://discord.gg/7sKKp7XQ2d) - Get help, share your agents, and discuss ideas
- [Follow @ActionbookHQ on X](https://x.com/ActionbookHQ) - Product updates and announcements

## Development

This is a monorepo using [pnpm](https://pnpm.io/) workspaces and [Turborepo](https://turborepo.com/).

### Prerequisites

- Node.js >= 18 (20+ recommended)
- pnpm >= 10
- PostgreSQL database (local or hosted like [Neon](https://neon.tech) / [Supabase](https://supabase.com))

### First-time Setup

1. Install dependencies:

```bash
pnpm install
```

2. Configure environment variables by copying `.env.example` to `.env` in the following packages:
   - `services/db`
   - `apps/api-service`
   - `services/action-builder` (optional, for recording)
   - `services/knowledge-builder` (optional, for knowledge extraction)

3. Run database migrations:

```bash
cd services/db && pnpm migrate
```

### Start the Development Server

```bash
pnpm dev
```

## Contributing

- **[Request a Website](https://actionbook.dev/request-website)** - Suggest websites you want Actionbook to index.
- **[Join the Waitlist](https://actionbook.dev)** - We are currently in private beta. Join if you are interested in contributing or using Actionbook.

## License

See [LICENSE](LICENSE) for the license details.

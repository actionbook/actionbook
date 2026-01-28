![Actionbook Cover](https://github.com/user-attachments/assets/85767111-a3ae-451f-a3e4-d625cf2b5710)

<div align="center">

### Actionbook

**The Action Playbook for Agents**
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
- [Usage Examples](#usage-examples)
- [Available Tools](#available-tools)
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

# Install browser (Chromium) for automation
actionbook browser install
```

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

| Method | Best For | Installation Time |
|--------|----------|-------------------|
| **CLI (Recommended)** | AI agents, browser automation, command line | < 1 minute |
| **MCP Server** | AI IDEs (Cursor, Claude Code, VS Code) | < 1 minute |
| **JavaScript SDK** | Custom agents, programmatic integration | < 2 minutes |

### Prerequisites

Before installing, make sure you have:

- ✅ **Node.js** >= v18.0.0 ([Download](https://nodejs.org))

> **💡 Tip:** Check your Node.js version with `node --version`
>
> **Note:** Actionbook is currently in open beta. No API key required!

---

### Option 1: CLI (Recommended)

Install the CLI globally to use Actionbook from the command line or with any AI agent.

```bash
npm install -g @actionbookdev/cli

# Install browser (Chromium) for automation
actionbook browser install
```

**Basic Commands:**

```bash
# Search for action manuals
actionbook search "airbnb search"

# Get action details by ID
actionbook get "site/airbnb.com/page/home/element/search-button"

# Browser automation
actionbook browser open airbnb.com
actionbook browser click '[data-testid="search-button"]'
actionbook browser fill 'input[name="location"]' 'Tokyo'
```

---

### Option 2: MCP Server

Use this option if you're working with an MCP-compatible AI IDE.

<details>
<summary><b>Cursor</b></summary>

Go to: `Settings` -> `Cursor Settings` -> `MCP` -> `Add new global MCP server`

Paste the following configuration:

```json
{
  "mcpServers": {
    "actionbook": {
      "command": "npx",
      "args": ["-y", "@actionbookdev/mcp@latest"]
    }
  }
}
```

</details>

<details>
<summary><b>Claude Code</b></summary>

Run the following command:

```bash
claude mcp add actionbook -- npx -y @actionbookdev/mcp@latest --api-key YOUR_API_KEY
```

</details>

<details>
<summary><b>VS Code</b></summary>

Add this to your VS Code settings (JSON):

```json
{
  "mcp": {
    "servers": {
      "actionbook": {
        "command": "npx",
        "args": ["-y", "@actionbookdev/mcp@latest"]
      }
    }
  }
}
```

</details>

<details>
<summary><b>Windsurf</b></summary>

Add this to your `~/.codeium/windsurf/mcp_config.json` file:

```json
{
  "mcpServers": {
    "actionbook": {
      "command": "npx",
      "args": ["-y", "@actionbookdev/mcp@latest"]
    }
  }
}
```

</details>

<details>
<summary><b>Claude Desktop</b></summary>

Add this to your `claude_desktop_config.json` file:

```json
{
  "mcpServers": {
    "actionbook": {
      "command": "npx",
      "args": ["-y", "@actionbookdev/mcp@latest"]
    }
  }
}
```

</details>

<details>
<summary><b>Cline</b></summary>

Go to: `Settings` -> `MCP Servers` -> `Add new MCP server`

```json
{
  "mcpServers": {
    "actionbook": {
      "command": "npx",
      "args": ["-y", "@actionbookdev/mcp@latest"]
    }
  }
}
```

</details>

<details>
<summary><b>Zed</b></summary>

Add this to your Zed settings.json:

```json
{
  "context_servers": {
    "actionbook": {
      "command": {
        "path": "npx",
        "args": ["-y", "@actionbookdev/mcp@latest"]
      }
    }
  }
}
```

</details>

<details>
<summary><b>JetBrains IDEs</b></summary>

Go to: `Settings` -> `Tools` -> `AI Assistant` -> `Model Context Protocol (MCP)`

```json
{
  "mcpServers": {
    "actionbook": {
      "command": "npx",
      "args": ["-y", "@actionbookdev/mcp@latest"]
    }
  }
}
```

</details>

<details>
<summary><b>Amazon Q Developer CLI</b></summary>

Add this to your `~/.aws/amazonq/mcp.json` file:

```json
{
  "mcpServers": {
    "actionbook": {
      "command": "npx",
      "args": ["-y", "@actionbookdev/mcp@latest"]
    }
  }
}
```

</details>

<details>
<summary><b>Warp</b></summary>

Go to: `Settings` -> `AI` -> `Manage MCP servers`

```json
{
  "actionbook": {
    "command": "npx",
    "args": ["-y", "@actionbookdev/mcp@latest", "--api-key", "YOUR_API_KEY"],
    "env": {},
    "working_directory": null,
    "start_on_launch": true
  }
}
```

</details>

<details>
<summary><b>Roo Code</b></summary>

Go to: `Settings` -> `MCP Servers` -> `Add new MCP server`

```json
{
  "mcpServers": {
    "actionbook": {
      "command": "npx",
      "args": ["-y", "@actionbookdev/mcp@latest"]
    }
  }
}
```

</details>

<details>
<summary><b>Augment Code</b></summary>

Go to: `Settings` -> `MCP Servers` -> `Add Server`

```json
{
  "mcpServers": {
    "actionbook": {
      "command": "npx",
      "args": ["-y", "@actionbookdev/mcp@latest"]
    }
  }
}
```

</details>

<details>
<summary><b>Trae</b></summary>

Go to: `Settings` -> `MCP Servers` -> `Add Server`

```json
{
  "mcpServers": {
    "actionbook": {
      "command": "npx",
      "args": ["-y", "@actionbookdev/mcp@latest"]
    }
  }
}
```

</details>

<details>
<summary><b>Gemini CLI</b></summary>

Add this to your `~/.gemini/settings.json` file:

```json
{
  "mcpServers": {
    "actionbook": {
      "command": "npx",
      "args": ["-y", "@actionbookdev/mcp@latest"]
    }
  }
}
```

</details>

<details>
<summary><b>Using Bun</b></summary>

```json
{
  "mcpServers": {
    "actionbook": {
      "command": "bunx",
      "args": ["@actionbookdev/mcp@latest"]
    }
  }
}
```

</details>

<details>
<summary><b>Using Deno</b></summary>

```json
{
  "mcpServers": {
    "actionbook": {
      "command": "deno",
      "args": [
        "run",
        "--allow-all",
        "npm:@actionbookdev/mcp"
      ]
    }
  }
}
```

</details>

---

### Option 3: JavaScript SDK

Use this option to integrate Actionbook directly into your custom AI agents built with any LLM framework.

**Step 1: Install the SDK**

```bash
# Using npm
npm install @actionbookdev/sdk

# Using pnpm
pnpm add @actionbookdev/sdk

# Using yarn
yarn add @actionbookdev/sdk

# Using bun
bun add @actionbookdev/sdk
```

**Step 2: Basic Usage**

```typescript
import { Actionbook } from '@actionbookdev/sdk'

// Initialize the client (no API key required during open beta)
const actionbook = new Actionbook()

// Search for action manuals
const results = await actionbook.searchActions('airbnb search')
console.log(`Found ${results.length} actions:`, results)

// Get a specific action by ID
const action = await actionbook.getActionById(results[0].id)
console.log('Action details:', action)

// Access the selectors
const selector = action.selectors.css ||
                 action.selectors.dataTestId ||
                 action.selectors.ariaLabel

console.log('Use this selector:', selector)
```

**Tool Definitions:**

Each method has `description` and `params` attached for easy integration with any LLM framework.

```typescript
import { Actionbook } from '@actionbookdev/sdk'

const actionbook = new Actionbook()

// Description
actionbook.searchActions.description  // "Search for action manuals by keyword"

// Params - JSON Schema format
actionbook.searchActions.params.json  // { type: "object", properties: { query: { type: "string" } }, required: ["query"] }

// Params - Zod format
actionbook.searchActions.params.zod   // z.object({ query: z.string() })
```

**Integration Examples:**

<details>
<summary><b>With Vercel AI SDK</b></summary>

```typescript
import { Actionbook } from '@actionbookdev/sdk'
import { generateText, tool } from 'ai'
import { openai } from '@ai-sdk/openai'

const actionbook = new Actionbook()

const { text } = await generateText({
  model: openai('gpt-4o'),
  tools: {
    searchActions: tool({
      description: actionbook.searchActions.description,
      parameters: actionbook.searchActions.params.zod,
      execute: async ({ query }) => actionbook.searchActions(query),
    }),
    getActionById: tool({
      description: actionbook.getActionById.description,
      parameters: actionbook.getActionById.params.zod,
      execute: async ({ id }) => actionbook.getActionById(id),
    }),
  },
  maxSteps: 5,
  prompt: 'Search for LinkedIn message actions and get the action manual',
})
```

</details>

<details>
<summary><b>With OpenAI SDK</b></summary>

```typescript
import { Actionbook } from '@actionbookdev/sdk'
import OpenAI from 'openai'

const actionbook = new Actionbook()
const openai = new OpenAI()

const tools: OpenAI.ChatCompletionTool[] = [
  {
    type: 'function',
    function: {
      name: 'searchActions',
      description: actionbook.searchActions.description,
      parameters: actionbook.searchActions.params.json,
    },
  },
  {
    type: 'function',
    function: {
      name: 'getActionById',
      description: actionbook.getActionById.description,
      parameters: actionbook.getActionById.params.json,
    },
  },
]

const completion = await openai.chat.completions.create({
  model: 'gpt-4o',
  tools,
  messages: [{ role: 'user', content: 'Search for Google login actions' }],
})
```

</details>

<details>
<summary><b>With Anthropic Claude SDK</b></summary>

```typescript
import { Actionbook } from '@actionbookdev/sdk'
import Anthropic from '@anthropic-ai/sdk'

const actionbook = new Actionbook()
const anthropic = new Anthropic()

const tools: Anthropic.Tool[] = [
  {
    name: 'searchActions',
    description: actionbook.searchActions.description,
    input_schema: actionbook.searchActions.params.json,
  },
  {
    name: 'getActionById',
    description: actionbook.getActionById.description,
    input_schema: actionbook.getActionById.params.json,
  },
]

const message = await anthropic.messages.create({
  model: 'claude-sonnet-4-20250514',
  max_tokens: 1024,
  tools,
  messages: [{ role: 'user', content: 'Search for Twitter post actions' }],
})
```

</details>

<details>
<summary><b>With Google Gemini SDK</b></summary>

```typescript
import { Actionbook } from '@actionbookdev/sdk'
import { GoogleGenAI } from '@google/genai'

const actionbook = new Actionbook()
const genai = new GoogleGenAI({ apiKey: process.env.GEMINI_API_KEY })

const tools = [
  {
    functionDeclarations: [
      {
        name: 'searchActions',
        description: actionbook.searchActions.description,
        parameters: actionbook.searchActions.params.json,
      },
      {
        name: 'getActionById',
        description: actionbook.getActionById.description,
        parameters: actionbook.getActionById.params.json,
      },
    ],
  },
]

const response = await genai.models.generateContent({
  model: 'gemini-2.0-flash',
  contents: [{ role: 'user', parts: [{ text: 'Search for YouTube upload actions' }] }],
  config: { tools },
})
```

</details>

<details>
<summary><b>With Stagehand</b></summary>

```typescript
import { Actionbook } from '@actionbookdev/sdk'
import { tool } from 'ai'

const actionbook = new Actionbook()

const agent = stagehand.agent({
  model: 'openai/gpt-4o',
  tools: {
    searchActions: tool({
      description: actionbook.searchActions.description,
      inputSchema: actionbook.searchActions.params.zod,
      execute: async ({ query }) => actionbook.searchActions(query),
    }),
    getActionById: tool({
      description: actionbook.getActionById.description,
      inputSchema: actionbook.getActionById.params.zod,
      execute: async ({ id }) => actionbook.getActionById(id),
    }),
  },
})

await agent.execute('Search for Airbnb booking actions and get the action manual')
```

</details>

---

## Examples

Explore real-world examples in the [playground](./playground) directory:

- [**stagehand-agent**](./playground/stagehand-agent) - Browser automation agent using Stagehand + AI SDK + Actionbook
- [**generate-scraper-script**](./playground/generate-scraper-script) - Generate accurate web scraper code with Claude Code
- [**actionbook-scraper**](./playground/actionbook-scraper) - Web scraper plugin using verified Actionbook selectors
- [**arxiv-viewer**](./playground/arxiv-viewer) - Access and read academic papers from arXiv
- [**rust-learner**](./playground/rust-learner) - Learn Rust language features and crate updates
- [**lib-rs-scraper**](./playground/lib-rs-scraper) - Scrape Rust crate information from lib.rs

## Available Tools

Actionbook MCP provides the following tools:

__`search_actions`__

Searches for available action manuals based on a query.

- `query` (required): The search query to find relevant action manuals (e.g., "airbnb search", "google login")

__`get_action_by_id`__

Retrieves a specific action manual by its ID, including DOM selectors and step-by-step instructions.

- `id` (required): The unique identifier of the action manual

## Stay tuned

Star Actionbook on Github to support and get latest information.

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

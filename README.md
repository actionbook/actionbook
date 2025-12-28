<div align="center">

# Actionbook

**The Playbook for AI Agents**
<br />
Make your agents act 10× faster with 100× token savings.
<br />
Powered by up-to-date action manuals and DOM cache.

[Website](https://actionbook.dev) · [GitHub](https://github.com/actionbook/actionbook)

</div>

<br />

## ❌ Without Actionbook

Building reliable browser agents is difficult and expensive:

- **Slow Execution:** Agents waste time parsing full HTML pages to find elements.
- **High Token Costs:** Sending entire DOM trees to LLMs consumes massive context windows.
- **Brittle Selectors:** Updates to website UIs break hardcoded selectors and agent logic immediately.
- **Hallucinations:** LLMs often guess incorrect actions when faced with complex, unstructured DOMs.

## ✅ With Actionbook

Actionbook places up-to-date action manuals with the relevant DOM selectors directly into your LLM's context.

- **10x Faster:** Agents access pre-computed "Action manuals" to know exactly what to do without exploring.
- **100x Token Savings:** Instead of whole HTML page, agents receive only related DOM elements in concise, semantic JSON definitions.
- **Resilient Automation:** Action manuals are maintained and versioned. If a site changes, the manual is updated, not your agent.
- **Universal Compatibility:** Works with any LLM (OpenAI, Anthropic, Gemini) and any AI operator framework.

## Installation

Actionbook provides two ways to integrate with your AI agents:

1. **MCP Server** - For MCP-compatible clients (Cursor, Claude Code, VS Code, etc.)
2. **JavaScript SDK** - For custom integrations with any LLM framework (Vercel AI SDK, OpenAI SDK, Claude SDK, etc.)

### Requirements

- Node.js >= v18.0.0
- Actionbook API Key (get yours at [actionbook.dev](https://actionbook.dev))

---

### Option 1: MCP Server

Use this option if you're working with an MCP-compatible client.

<details>
<summary><b>Cursor</b></summary>

Go to: `Settings` -> `Cursor Settings` -> `MCP` -> `Add new global MCP server`

Paste the following configuration:

```json
{
  "mcpServers": {
    "actionbook": {
      "command": "npx",
      "args": ["-y", "@actionbookdev/mcp@latest", "--api-key", "YOUR_API_KEY"]
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
        "args": ["-y", "@actionbookdev/mcp@latest", "--api-key", "YOUR_API_KEY"]
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
      "args": ["-y", "@actionbookdev/mcp@latest", "--api-key", "YOUR_API_KEY"]
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
      "args": ["-y", "@actionbookdev/mcp@latest", "--api-key", "YOUR_API_KEY"]
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
      "args": ["-y", "@actionbookdev/mcp@latest", "--api-key", "YOUR_API_KEY"]
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
        "args": ["-y", "@actionbookdev/mcp@latest", "--api-key", "YOUR_API_KEY"]
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
      "args": ["-y", "@actionbookdev/mcp@latest", "--api-key", "YOUR_API_KEY"]
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
      "args": ["-y", "@actionbookdev/mcp@latest", "--api-key", "YOUR_API_KEY"]
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
      "args": ["-y", "@actionbookdev/mcp@latest", "--api-key", "YOUR_API_KEY"]
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
      "args": ["-y", "@actionbookdev/mcp@latest", "--api-key", "YOUR_API_KEY"]
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
      "args": ["-y", "@actionbookdev/mcp@latest", "--api-key", "YOUR_API_KEY"]
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
      "args": ["-y", "@actionbookdev/mcp@latest", "--api-key", "YOUR_API_KEY"]
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
      "args": ["@actionbookdev/mcp@latest", "--api-key", "YOUR_API_KEY"]
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
        "npm:@actionbookdev/mcp",
        "--api-key",
        "YOUR_API_KEY"
      ]
    }
  }
}
```

</details>

---

### Option 2: JavaScript SDK

Use this option to integrate Actionbook directly into your custom AI agents built with any LLM framework.

**Install the SDK:**

```bash
npm install @actionbookdev/sdk
```

**Basic Usage:**

```typescript
import { Actionbook } from '@actionbookdev/sdk'

const client = new Actionbook({ apiKey: 'YOUR_API_KEY' })

// Search for action manuals
const results = await client.searchActions('airbnb search')

// Get a specific action by ID
const action = await client.getActionById('site/airbnb.com/page/search/element/search-button')
```

**Tool Definitions:**

Each method has `description` and `params` attached for easy integration with any LLM framework.

```typescript
import { Actionbook } from '@actionbookdev/sdk'

const client = new Actionbook({ apiKey: 'YOUR_API_KEY' })

// Description
client.searchActions.description  // "Search for action manuals by keyword"

// Params - JSON Schema format
client.searchActions.params.json  // { type: "object", properties: { query: { type: "string" } }, required: ["query"] }

// Params - Zod format
client.searchActions.params.zod   // z.object({ query: z.string() })
```

**Integration Examples:**

<details>
<summary><b>With Vercel AI SDK</b></summary>

```typescript
import { Actionbook } from '@actionbookdev/sdk'
import { generateText, tool } from 'ai'
import { openai } from '@ai-sdk/openai'

const actionbook = new Actionbook({ apiKey: process.env.ACTIONBOOK_API_KEY })

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

const actionbook = new Actionbook({ apiKey: process.env.ACTIONBOOK_API_KEY })
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

const actionbook = new Actionbook({ apiKey: process.env.ACTIONBOOK_API_KEY })
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

const actionbook = new Actionbook({ apiKey: process.env.ACTIONBOOK_API_KEY })
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

const actionbook = new Actionbook({ apiKey: process.env.ACTIONBOOK_API_KEY })

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

## Available Tools

Actionbook MCP provides the following tools:

__`search_actions`__

Searches for available action manuals based on a query.

- `query` (required): The search query to find relevant action manuals (e.g., "airbnb search", "google login")

__`get_action_by_id`__

Retrieves a specific action manual by its ID, including DOM selectors and step-by-step instructions.

- `id` (required): The unique identifier of the action manual

## Stay tuned

Star Actionbook on Github to support and get lastest information.

![star-actionbook](https://github.com/user-attachments/assets/940c1825-99a8-46e2-8d0f-cc96c43e922f)

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

We are currently in private beta. If you are interested in contributing or using Actionbook, please [join the waitlist](https://actionbook.dev).

## License

See [LICENSE](LICENSE) for the license details.

# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Always use context7 when I need code generation, setup or configuration steps, or library/API documentation. This means you should automatically use the Context7 MCP tools to resolve library id and get library docs without me having to explicitly ask.

## Project Overview

**Actionbook** is a website UI Action service platform that provides AI Agents with accurate, real-time website operation information (element selectors, operation methods, page structure). The core value proposition: "Let AI Agents precisely operate any website without repeatedly learning page structures."

Actionbook bridges AI Agents and website Action libraries through the MCP protocol, so Agents can directly obtain verified selectors and operation methods without parsing pages each time.

## Architecture

### Domain Model Hierarchy

```
Site → Page → Element → ElementAction
                    ↘ Scenario → ScenarioStep
```

- **Site**: Website domain with metadata, health score, tags
- **Page**: Functional page type with URL patterns
- **Element**: Interactive UI element with semantic ID
- **ElementAction**: Selectors (css, xpath, ariaLabel, dataTestId) and allowed methods (click, type, etc.)
- **Scenario**: Complete user operation flow composed of multiple steps

### Module Structure

```
actionbook/
├── packages/
│   ├── js-sdk/             # @actionbookdev/sdk - JavaScript SDK with tool definitions
│   ├── mcp/                # @actionbookdev/mcp - MCP Server (standalone, publishable to npm)
│   └── tools-ai-sdk/       # AI SDK tools integration (placeholder)
├── apps/
│   ├── website/            # Next.js 16 landing page (Vercel deployment)
│   ├── api-service/        # REST API (Vercel deployment)
│   ├── api-server/         # API server (placeholder)
│   └── docs/               # Product documentation (placeholder)
├── services/
│   ├── db/                 # @actionbookdev/db - Database Schema + Types (Drizzle ORM)
│   ├── action-builder/     # Action recording, validation, Eval (local)
│   ├── knowledge-builder/  # Scenario knowledge extraction, Eval (local)
│   └── common/             # Shared internal packages for services
├── playground/             # Demo and example projects
└── old_projects/           # Legacy/archived projects
```

### Core MCP Tools

1. `search_actions` - Search for actions by keyword
2. `get_action_by_id` - Get action content by ActionId

ActionId format: `site/{domain}/page/{pageType}/element/{semanticId}`

### Data Flow

1. Agent calls MCP tool `search_actions` or `get_action_by_id`
2. MCP Server forwards to API Service
3. API Service queries PostgreSQL
4. Returns ActionMeta/ActionContent to Agent
5. Agent uses returned selectors to execute operations

## Technology Stack

- **Language**: TypeScript 5.x, Node.js 20+
- **Package Manager**: pnpm (monorepo)
- **Build System**: Turborepo (caching, parallel builds)
- **MCP Protocol**: @modelcontextprotocol/sdk
- **API Service**: Next.js (App Router) on Vercel
- **Browser Automation**: Playwright, Stagehand
- **Database**: PostgreSQL (Neon/Supabase) with Drizzle ORM
- **LLM**: OpenRouter SDK (multi-model support)
- **Validation**: Zod schemas

## Development Commands

```bash
# Install dependencies
pnpm install

# Start local database
docker-compose up -d postgres

# Run database migrations
cd services/db && pnpm migrate

# Development mode (all services)
pnpm dev

# Build all (with caching via Turborepo)
pnpm build

# Run tests
pnpm test

# Lint all packages
pnpm lint

# Clean all build outputs
pnpm clean
```

### Turborepo Commands

```bash
# Build specific package
pnpm build --filter=@actionbookdev/sdk

# Build package and its dependencies
pnpm build --filter=@actionbookdev/mcp...

# Run dev for specific apps
pnpm dev --filter=@actionbookdev/api-service --filter=actionbook-home
```

### Package-Specific Commands

**JavaScript SDK (packages/js-sdk)**:

```bash
cd packages/js-sdk
pnpm build            # Build the SDK
pnpm test             # Run tests
```

**MCP Server (packages/mcp)**:

```bash
cd packages/mcp
pnpm build            # Build MCP server
pnpm test             # Run tests
```

**Database (services/db)**:

```bash
cd services/db
pnpm build            # Build with tsup
pnpm migrate          # Run Drizzle migrations
pnpm migrate:generate # Generate new migration
pnpm migrate:push     # Safe push (with confirmation)
pnpm studio           # Open Drizzle Studio
```

**Website (apps/website)**:

```bash
cd apps/website
pnpm dev              # Start dev server at localhost:3000
pnpm build            # Production build
pnpm lint             # Run ESLint
```

## Shared Database Package (@actionbookdev/db)

The `services/db` package provides shared schema and types for all services.

### Tables

| Table             | Description                                              |
| ----------------- | -------------------------------------------------------- |
| `sources`         | Data sources (website) information                       |
| `documents`       | Crawled web documents                                    |
| `chunks`          | Document chunks with vector embeddings (1536 dimensions) |
| `crawlLogs`       | Crawl task execution logs                                |
| `recording_tasks` | Recording tasks                                          |
| `recording_steps` | Recording steps                                          |
| `pages`           | Page types                                               |
| `elements`        | UI elements                                              |

### Usage in other services

Add dependency in `package.json`:

```json
{
  "dependencies": {
    "@actionbookdev/db": "workspace:*"
  }
}
```

Import in code:

```typescript
import { getDb, sources, documents, chunks, crawlLogs } from '@actionbookdev/db'
import type { Source, Document, Chunk, CrawlLog } from '@actionbookdev/db'

const db = getDb()
const allSources = await db.select().from(sources)
```

### Database commands

```bash
cd services/db
pnpm migrate      # Run migrations
pnpm studio       # Open Drizzle Studio
```

## Key Design Decisions

1. **SDK + MCP separation**: `@actionbookdev/sdk` provides core types and tool definitions; `@actionbookdev/mcp` depends on SDK for MCP protocol implementation
2. **Builder/API separation**: Builders run locally, output to database; API serves from Vercel
3. **Types from DB schema**: Domain types are inferred from Drizzle schema in services/db
4. **Eval is co-located**: Each builder contains its own evaluation logic
5. **Query-only MCP**: MCP provides queries only; Agent executes operations itself

## Environment Variables

Each package manages its own `.env` file. Check the `.env.example` in each package:

| Package                      | Variables                                                |
| ---------------------------- | -------------------------------------------------------- |
| `services/db`                | `DATABASE_URL`                                           |
| `services/action-builder`    | `DATABASE_URL`, `OPENROUTER_API_KEY`, Stagehand settings |
| `services/knowledge-builder` | `DATABASE_URL`, `OPENAI_API_KEY`, proxy settings         |
| `apps/api-service`           | `DATABASE_URL`, `OPENAI_API_KEY`                         |

## Development Workflow

1. This is a pnpm workspace with Turborepo - always use `pnpm` instead of `npm` or `yarn`
2. Node.js 18+ required (20+ recommended), pnpm 10+ required
3. Copy `.env.example` to `.env` in each package you're working with
4. Check for existing CLAUDE.md files in subdirectories for package-specific guidance
5. Follow existing code patterns and conventions in each workspace

## File Organization

**IMPORTANT**: When creating files during development, follow these conventions:

### Test Scripts

TODO

### Documentation

All documentation files created during implementation should be placed in:

```
.docs/
```

This includes:

- Architecture documentation
- Implementation guides
- API documentation
- Design decisions
- Troubleshooting guides

**Note**: Product documentation for end users should go in `apps/docs/`.

## Published Packages

The following packages are published to npm:

| Package           | npm Name             | Description                                       |
| ----------------- | -------------------- | ------------------------------------------------- |
| `packages/js-sdk` | `@actionbookdev/sdk` | Core SDK with types and tool definitions          |
| `packages/mcp`    | `@actionbookdev/mcp` | MCP Server implementation (CLI: `actionbook-mcp`) |

## Git Commit Message Convention

**IMPORTANT**: This is a monorepo. All commit messages MUST follow this format:

```
[scope]type: description

[optional body]

[optional footer]
```

- `[scope]`: The workspace/package path in square brackets, or `[root]` for root-level files
  - Workspace examples: `[packages/node-sdk]`, `[apps/api-server]`, `[playground/quickstart-demo]`
  - Root-level: `[root]` (for files like CLAUDE.md, package.json, tsconfig.json, etc.)
- `type`: Conventional commit type (`feat`, `fix`, `docs`, `refactor`, `test`, `chore`, etc.)
- `description`: Brief description of the change

**Examples**:

```
[packages/node-sdk]fix: correct ESM export path in package.json
[apps/api-server]feat: add new container management endpoint
[playground/quickstart-demo]docs: update README with setup instructions
[apps/website]refactor: migrate to new API client
[root]docs: add Git commit message convention
[root]chore: update pnpm-workspace.yaml
```

**Multi-package changes**: Use the primary affected package as scope.

## Architecture Overview

- **Website**: Next.js 16 landing page with waitlist functionality
- **API Service**: Next.js serverless functions providing REST API
- **MCP Server**: Model Context Protocol server for AI agent integration
- **Database**: PostgreSQL (Neon) with Drizzle ORM for schema management

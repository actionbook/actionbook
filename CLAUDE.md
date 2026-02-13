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
│   ├── cli/                # @actionbookdev/cli - Command line interface
│   └── tools-ai-sdk/       # @actionbookdev/tools-ai-sdk - Vercel AI SDK tools integration
├── playground/             # Demo and example projects
│   ├── rust-learner/       # Rust learner plugin example
│   └── stagehand-agent/    # Stagehand agent example
└── eval/                   # Evaluation framework
```

### Core MCP Tools

1. `search_actions` - Search for actions by keyword
2. `get_action_by_id` - Get action content by ActionId

ActionId format: `site/{domain}/page/{pageType}/element/{semanticId}`

### Data Flow

1. Agent calls MCP tool `search_actions` or `get_action_by_id`
2. MCP Server forwards to API Service
3. API Service queries database
4. Returns ActionMeta/ActionContent to Agent
5. Agent uses returned selectors to execute operations

## Technology Stack

- **Language**: TypeScript 5.x, Node.js 20+
- **Package Manager**: pnpm (monorepo)
- **Build System**: Turborepo (caching, parallel builds)
- **MCP Protocol**: @modelcontextprotocol/sdk
- **AI SDK**: Vercel AI SDK
- **Validation**: Zod schemas

## Development Commands

```bash
# Install dependencies
pnpm install

# Development mode (all packages)
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

**CLI (packages/cli)**:

```bash
cd packages/cli
pnpm build            # Build CLI
pnpm test             # Run tests
```

**AI SDK Tools (packages/tools-ai-sdk)**:

```bash
cd packages/tools-ai-sdk
pnpm build            # Build AI SDK tools
pnpm test             # Run tests
```

## Key Design Decisions

1. **SDK + MCP separation**: `@actionbookdev/sdk` provides core types and tool definitions; `@actionbookdev/mcp` depends on SDK for MCP protocol implementation
2. **Query-only MCP**: MCP provides queries only; Agent executes operations itself
3. **AI SDK integration**: `@actionbookdev/tools-ai-sdk` provides Vercel AI SDK compatible tools

## Environment Variables

Each package manages its own `.env` file. Check the `.env.example` in each package.

## Development Workflow

1. This is a pnpm workspace with Turborepo - always use `pnpm` instead of `npm` or `yarn`
2. Node.js 18+ required (20+ recommended), pnpm 10+ required
3. Copy `.env.example` to `.env` in each package you're working with
4. Check for existing CLAUDE.md files in subdirectories for package-specific guidance
5. Follow existing code patterns and conventions in each workspace

## File Organization

**IMPORTANT**: When creating files during development, follow these conventions:

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

## Published Packages

The following packages are published to npm:

| Package                | npm Name                    | Description                                       |
| ---------------------- | --------------------------- | ------------------------------------------------- |
| `packages/js-sdk`      | `@actionbookdev/sdk`        | Core SDK with types and tool definitions          |
| `packages/mcp`         | `@actionbookdev/mcp`        | MCP Server implementation (CLI: `actionbook-mcp`) |
| `packages/cli`         | `@actionbookdev/cli`        | Command line interface                            |
| `packages/tools-ai-sdk`| `@actionbookdev/tools-ai-sdk` | Vercel AI SDK tools integration                 |

## Git Commit Message Convention

**IMPORTANT**: This is a monorepo. All commit messages MUST follow this format:

```
[scope]type: description

[optional body]

[optional footer]
```

- `[scope]`: The workspace/package path in square brackets, or `[root]` for root-level files
  - Workspace examples: `[packages/js-sdk]`, `[packages/mcp]`, `[playground/rust-learner]`
  - Root-level: `[root]` (for files like CLAUDE.md, package.json, tsconfig.json, etc.)
- `type`: Conventional commit type (`feat`, `fix`, `docs`, `refactor`, `test`, `chore`, etc.)
- `description`: Brief description of the change

**Examples**:

```
[packages/js-sdk]fix: correct ESM export path in package.json
[packages/mcp]feat: add new tool for action search
[playground/rust-learner]docs: update README with setup instructions
[root]docs: update CLAUDE.md
[root]chore: update pnpm-workspace.yaml
```

**Multi-package changes**: Use the primary affected package as scope.

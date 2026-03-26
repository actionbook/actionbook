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

## Release Workflow

This project uses **Changesets** for versioning and **GitHub Actions** for automated publishing. Manual publishing is not supported — all releases go through CI.

### Developer Steps

```bash
# 1. After making changes, create a changeset
pnpm changeset
# Select affected packages, semver bump type (major/minor/patch), and description

# 2. Commit the generated .changeset/*.md file along with your changes
git add .changeset/*.md
git commit -m "[scope]feat: description"

# 3. Push/merge to main — CI handles the rest
```

### CI Release Process (Two-Phase)

**Phase 1 — Version PR**: When changesets are detected on `main`, CI automatically creates a "Version Packages" PR that updates package versions, CHANGELOGs, and runs `scripts/sync-versions.js` to keep derived manifests in sync.

**Phase 2 — Publish**: After the Version PR is merged (no pending changesets), CI compares local versions against the npm registry and publishes only changed packages:

| Type | Packages | Publish Target |
|------|----------|----------------|
| JS packages | `sdk`, `mcp`, `tools-ai-sdk`, `json-ui` | npm (`npm publish --provenance`) |
| CLI | `@actionbookdev/cli` + 6 platform packages | Rust cross-compile → npm + GitHub Release |
| Extension | `actionbook-extension` | ZIP → GitHub Release |
| Dify Plugin | `dify-plugin` | `.difypkg` → GitHub Release |

### Version Sync

`scripts/sync-versions.js` runs automatically after `changeset version` to sync:
- CLI main version → 6 platform-specific `package.json` files
- Extension `package.json` → `manifest.json`
- Dify plugin `package.json` → `manifest.yaml` + `pyproject.toml`

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

## Actionbook CLI

In `packages/cli*` or `packages/actionbook-rs`, should follow the rules


### Product Principles

- Stateless interface, stateful runtime.                                                                                                                                                                                    
The CLI interface facing agents is completely stateless — every command explicitly addresses via --session and --tab, is self-contained, and depends on no side effects from prior commands. The daemon itself is stateful,  
holding the CDP connection pool and session/tab registry. The key distinction: the agent doesn't need to track any state; the daemon manages it on its behalf.                                                               
- Absolute-path addressing, like a filesystem.                                                                                                                                                                               
The core analogy: humans open a file in an IDE before editing; agents call write(full_path, content) directly. All per-tab commands must include --session + --tab — omitting either is an error. There is no implicit       
"current tab" concept. This eliminates global locks and makes multi-tab parallel operations a first-class citizen.                                                                                                           
- Designed for LLM consumers, not humans.                                                                                                                                                                                    
Output defaults to compact text rather than JSON, because LLMs consume tokens more efficiently that way. Every response carries a [session tab] url prefix so the agent always knows its context. Short IDs (s0, t3) replace 
UUIDs, compressing addressing from 40+ tokens down to 3–4.                                                                                                                                                                   
- Errors as guidance.                                                                                                                                                                                                        
Every error response includes a hint field telling the agent what to do next. For example, SESSION_NOT_FOUND suggests run browser launch. This reduces the tokens agents waste on error recovery.                            

### Architecture

- **Three-layer decoupling.** CLI layer, Daemon layer, and Browser layer are independent, interacting only through protocols and trait boundaries. CLI layer handles argument parsing and output formatting only — no browser access. Daemon layer handles IPC routing and connection management only — no command semantics. Browser layer abstracts backends via traits — agnostic to whether commands come from CLI or daemon. Anti-pattern: calling `chromiumoxide::Browser::connect()` then `println!` directly in a command handler — that couples all three layers.

---

### Rust Engineering Constraints

- **Use mature crates, but stay vigilant.** clap derive for CLI, figment for config, thiserror for errors. Be cautious with chromiumoxide — it's 0.8, not 1.0. Its full CDP codegen causes slow compilation and binary bloat. If it falls behind Chrome's CDP evolution, have a fallback plan: thin WS client + serde_json::Value. The current Cargo.toml depending on both chromiumoxide and tokio-tungstenite is a redundancy signal — chromiumoxide uses tungstenite internally. If you also need raw WS, its abstraction is leaking.
- **Phase out async-trait, except for dyn Trait.** Rust 1.75+ natively supports async fn in trait (RPITIT) — prefer it for new code. However, BrowserBackend requires dynamic dispatch (`Box<dyn BrowserBackend>`), which still needs async-trait. Rule: use RPITIT internally, keep async-trait only for dyn-exposed interfaces.
- **Feature flags must gate compilation.** The current `stealth = []` is an empty feature — stealth code is always compiled. Use `#[cfg(feature = "stealth")]` to gate entire modules. camoufox does this correctly (gates thirtyfour dependency); stealth should follow.
- **panic = "abort" means Drop won't run.** Daemon socket/PID file cleanup cannot rely on Drop guards. Must use signal handlers (SIGTERM/SIGINT) for explicit cleanup. `catch_unwind` is unavailable — all errors must go through Result.
- **Graceful shutdown is mandatory.** Register `tokio::signal` for SIGINT/SIGTERM. On signal, daemon must close controlled browser instances and clean up socket/PID files to prevent zombie processes. This pairs with the panic=abort constraint — Drop cannot be relied on for cleanup.
- **opt-level = "z" is the right choice.** For a network-IO-bound CLI, CPU is rarely the bottleneck. If snapshot parsing or fingerprint generation shows performance issues, confirm with criterion benchmarks first, then consider splitting hot functions into a separate crate with a different opt-level. Don't change the whole profile because of one slow function.
- **Use edition 2024.** MSRV: latest stable minus one. This is a CLI tool, not a library depended on by other crates.
- **No blocking IO in async context.** Daemon hot paths (socket read/write, CDP message processing) must use `tokio::fs`. Cold paths (config loading, PID files) may use std::fs, but that's a deliberate tradeoff, not an oversight.
- **Keep terminal UI deps minimal.** console, indicatif, dialoguer are sufficient. Don't also pull in colored (overlaps with console). This is a CLI tool, not a TUI app.
- **No unsafe outside FFI.** The only permitted unsafe usages are Native Messaging cross-process communication and libc signal handlers.

---

### Output & Errors

- **Dual-channel output.** stdout for machines, stderr for humans. When `--json` is set, stdout must emit only valid JSON — zero pollution. Logs, progress, warnings all go to stderr. Anti-pattern: printing `Starting browser...` to stdout in `--json` mode, breaking `jq` parsing.
- **Errors must be typed.** Every error has a distinct variant name and machine-readable code (e.g. `browser_not_found`, `cdp_connection_failed`). Library layer uses thiserror; CLI top-level may use anyhow for propagation, but bare `Err("something failed".into())` or `anyhow!` is forbidden in library code. AI agents rely on error codes for retry strategy — string errors are useless.
- **Flags use kebab-case, env vars use ACTIONBOOK_ prefix.** Every global flag must support both CLI and env input. clap's `#[arg(env = "...")]` handles this directly.
- **Config precedence:** CLI flag > env var (`ACTIONBOOK_*`) > config file (`~/.actionbook/config.toml`) > defaults. figment natively supports this chain.
- **Use tracing, not log.** In daemon mode, structured logs must carry `session_id` and `profile_name` fields — otherwise concurrent sessions produce indistinguishable logs. Level controlled via `RUST_LOG` or `--verbose`.

---

### Security & Feature Boundaries

- **CDP method security levels.** L1: read-only (screenshot, DOM read). L2: modification (click, input, navigation). L3: high-risk (file download, permission grant). L3 is denied by default in Extension Bridge mode. Anti-pattern: no level enforcement in Extension Bridge, allowing `Browser.grantPermissions` — attacker gains camera access via malicious page.
- **Stealth off by default.** Anti-detection is opt-in. Users must explicitly enable it. Anti-pattern: stealth on by default causes non-reproducible E2E tests — User-Agent differs on every run.
- **Binary size is a hard constraint.** Target < 10MB (stripped). Means: `opt-level="z"` + LTO + strip + `panic="abort"` + feature-gate non-core modules. Periodically run `cargo bloat` to track size contributors.

## gstack

### Available Skills

- `/office-hours` - Brainstorm and explore ideas
- `/plan-ceo-review` - CEO/founder-mode plan review
- `/plan-eng-review` - Engineering manager plan review
- `/plan-design-review` - Design plan review
- `/design-consultation` - Create a design system
- `/review` - Pre-landing PR review
- `/ship` - Ship workflow (merge, test, bump, PR)
- `/land-and-deploy` - Land and deploy changes
- `/canary` - Canary deployment
- `/benchmark` - Run benchmarks
- `/qa` - QA testing
- `/qa-only` - QA testing (test only, no fixes)
- `/design-review` - Visual design audit
- `/setup-browser-cookies` - Set up browser cookies
- `/setup-deploy` - Set up deployment
- `/retro` - Weekly engineering retrospective
- `/investigate` - Debug and investigate errors
- `/document-release` - Post-ship documentation
- `/codex` - Adversarial code review
- `/careful` - Production/live systems safety mode
- `/freeze` - Scope edits to one module/directory
- `/guard` - Maximum safety mode
- `/unfreeze` - Remove edit restrictions
- `/gstack-upgrade` - Upgrade gstack to latest version

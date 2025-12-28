# Repository Guidelines

Always use context7 when I need code generation, setup or configuration steps, or library/API documentation. This means you should automatically use the Context7 MCP tools to resolve library id and get library docs without me having to explicitly ask.

## Project Structure & Module Organization

Actionbook is a pnpm + Turborepo monorepo.

- `apps/`: Next.js apps (`apps/website`, `apps/api-service`).
- `packages/`: publishable libraries (`packages/js-sdk`, `packages/mcp`).
- `services/`: internal services (`services/db`, `services/action-builder`, `services/knowledge-builder`, `services/knowledge-builder-any` in Rust).
- `playground/`: demos and experiments.
- Docs live in `.docs/` for internal notes; end-user docs go in `apps/docs/` when applicable.
- Static assets: `apps/website/public`; migrations: `services/db/migrations`.

## Architecture Overview

Core domain model: `Site -> Page -> Element -> ElementAction`, plus `Scenario -> ScenarioStep` for flows. MCP exposes `search_actions` and `get_action_by_id`; ActionId format is `site/{domain}/page/{pageType}/element/{semanticId}`.

## Build, Test, and Development Commands

- `pnpm install`: install dependencies (use pnpm only).
- `pnpm dev`: run all dev tasks.
- `pnpm build`: build all packages/apps; `pnpm build --filter=@actionbookdev/mcp...` builds a package + deps.
- `pnpm test`: run workspace tests; `pnpm test --filter=<workspace>` for scoped runs.
- `pnpm lint`: lint packages with ESLint.
- `cd services/db && pnpm migrate`: apply DB migrations.

## Coding Style & Naming Conventions

TypeScript/TSX and Rust; follow existing tsconfig/eslint rules per package. Use `camelCase` for functions/vars, `PascalCase` for types/classes/components, and `kebab-case` for file names (e.g., `build-task-worker.ts`). Prefer small, focused modules and keep changes scoped to one workspace when possible.

## Testing Guidelines

Vitest is standard for TS packages (`packages/mcp`, `packages/js-sdk`, `services/action-builder`, `apps/api-service`). Tests live in `src/` or `test/` and use `*.test.ts`, with `.ut` and `.it` suffixes for unit/integration. For Rust (`services/knowledge-builder-any`), run `cargo test`.

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

## Configuration & Secrets

Copy `.env.example` to `.env` in packages you run locally (`services/db`, `apps/api-service`, `services/action-builder`, `services/knowledge-builder`). Common vars include `DATABASE_URL` and LLM API keys. Never commit secrets.

## Agent-Specific Notes

Read `CLAUDE.md`, `GEMINI.md`, and any package-level `CLAUDE.md` before changing code. Use Context7 for library/API docs when generating code, setup, or configuration steps.

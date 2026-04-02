# Contributing to Actionbook

Thank you for your interest in contributing to Actionbook. This repository contains the public CLI, SDK, MCP server, JSON UI, docs, and example projects. The internal build pipeline now lives outside this repo.

## Ways to Contribute

- **Report Bugs** - Use our [bug report template](https://github.com/actionbook/actionbook/issues/new?template=bug-report.yml)
- **Propose Features** - Use our [feature request template](https://github.com/actionbook/actionbook/issues/new?template=feature-request.yml)
- **Improve Documentation** - Help us improve docs, README files, or code comments
- **Submit Code** - Fix bugs, implement features, or improve performance
- **Request Website Support** - Suggest new websites to add action manuals for
- **Ask Questions** - Use our [question template](https://github.com/actionbook/actionbook/issues/new?template=question.yml)

## Development Setup

### Prerequisites

Before you begin, ensure you have the following installed:

- **Node.js** 20+
- **pnpm** 10+
- **Rust** stable (required for `packages/cli`)
- **Git**

### Fork and Clone

1. Fork the repository on GitHub.
2. Clone your fork locally:

```bash
git clone https://github.com/YOUR_USERNAME/actionbook.git
cd actionbook
```

3. Add the upstream repository:

```bash
git remote add upstream https://github.com/actionbook/actionbook.git
```

### Install Dependencies

```bash
pnpm install
```

### Common Local Checks

Run the checks that match the package you changed instead of trying to run every tool in the monorepo:

```bash
pnpm --filter @actionbookdev/sdk test
pnpm --filter @actionbookdev/mcp build
pnpm --filter @actionbookdev/tools-ai-sdk test
pnpm --filter @actionbookdev/json-ui build
cargo test --manifest-path packages/cli/Cargo.toml
```

## Project Structure

Actionbook is a monorepo managed with [pnpm](https://pnpm.io/) workspaces and [Turborepo](https://turborepo.com/).

```text
actionbook/
├── packages/
│   ├── cli/             # Rust CLI + npm wrapper
│   ├── js-sdk/          # @actionbookdev/sdk
│   ├── mcp/             # @actionbookdev/mcp
│   ├── tools-ai-sdk/    # AI SDK integration
│   ├── json-ui/         # JSON UI renderer
│   └── dify-plugin/     # Dify integration
├── docs/                # Product documentation source
├── playground/          # Example projects and experiments
└── scripts/             # Repository maintenance scripts
```

## Commit Message Convention

All commit messages must follow this format:

```text
[scope]type: description
```

### Format Rules

- **`[scope]`**: The workspace path in square brackets, or `[root]` for root-level files
  - Examples: `[packages/js-sdk]`, `[packages/mcp]`, `[packages/cli]`, `[root]`
- **`type`**: Conventional commit type
  - `feat` - New feature
  - `fix` - Bug fix
  - `docs` - Documentation changes
  - `refactor` - Code refactoring
  - `test` - Adding or updating tests
  - `chore` - Maintenance tasks
  - `perf` - Performance improvements
  - `style` - Formatting-only changes
- **`description`**: Brief description of the change in lowercase, with no trailing period

### Examples

```bash
[packages/js-sdk]feat: add site filter to search actions
[packages/mcp]fix: align cli output with sdk schema
[packages/cli]refactor: simplify browser session lifecycle
[root]docs: update public repo contribution guide
```

## Coding Standards

### TypeScript

- Use TypeScript strict mode where it already exists.
- Prefer explicit types over `any`.
- Use [Zod](https://zod.dev/) for runtime validation and schema definition where applicable.

### Naming Conventions

- **Files**: `kebab-case.ts`
- **Components**: `PascalCase.tsx`
- **Functions**: `camelCase`
- **Constants**: `UPPER_SNAKE_CASE`
- **Types/Interfaces**: `PascalCase`

### File Organization

- **Development documentation**: Place in `.docs/`
- **Product documentation**: Place in `docs/`
- **Tests**: Keep them close to the affected package and use the package's existing conventions

## Pull Request Process

Before opening a pull request:

1. Create a branch from `main`.
2. Keep the change scoped to one problem.
3. Run the relevant checks for the packages you touched.
4. Summarize validation steps and any user-facing risk in the PR description.

## Testing Guidelines

- Prefer targeted package-level checks over broad monorepo runs.
- For JavaScript and TypeScript packages, use `pnpm --filter <package> <script>`.
- For Rust CLI changes, use `cargo test --manifest-path packages/cli/Cargo.toml`.
- If a change only affects docs or metadata, say so explicitly in the PR.

## Community Guidelines

- Follow the [Code of Conduct](CODE_OF_CONDUCT.md).
- Keep issues and pull requests concise and reproducible.
- Do not include private infrastructure details in public issues or PRs.

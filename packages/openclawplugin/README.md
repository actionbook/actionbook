# packages/openclawplugin

Design-only package placeholder for the future Actionbook → OpenClaw plugin integration.

## Current state

This directory intentionally contains **documentation only** for CUE-671.
No runtime package files or implementation code are included yet.

## Planned purpose

`packages/openclawplugin` is intended to become the Actionbook-owned package that
exposes Actionbook capability to OpenClaw as first-class tools.

For the MVP, the package is planned to expose only two tools:

- `actionbook_search`
- `actionbook_get`

## Why this package exists

The integration strategy settled during research is:

- **OpenClaw** remains the browser execution layer
- **Actionbook** becomes the low-token knowledge/query layer

Therefore this package is not intended to replace browser execution. Instead, it
will provide Actionbook knowledge to OpenClaw in a plugin-native way.

## What is in scope for the future implementation

1. A plugin-facing package layout under `packages/openclawplugin`
2. Search/get tool contracts designed for OpenClaw consumption
3. Stable, deterministic, machine-friendly outputs
4. Internal usage of `@actionbookdev/sdk`

## What is explicitly out of scope for the MVP

- `list_sources`
- `search_sources`
- browser execution
- selector -> `ref(uid)` bridge
- browser hook integration
- automatic runtime routing

## Documents in this directory

- `PLAN.md` — end-to-end package plan
- `DECISIONS.md` — confirmed decisions and rationale
- `TEST-PLAN.md` — planned validation and test coverage

## Next step when implementation resumes

When PM reopens implementation, this directory should be converted from
document-only to a real package by adding:

- `package.json`
- `openclaw.plugin.json`
- `src/index.ts`
- `src/lib/*`
- `src/tools/*`
- tests

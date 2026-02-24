---
"@actionbookdev/cli": patch
---

Refine `actionbook setup` behavior for agent and non-interactive workflows:

- remove `--agent-mode` and keep setup targeting via `--target`
- keep `--target` quick mode only when used alone
- run full setup when `--target` is combined with setup flags (for example `--non-interactive`, `--browser`, `--api-key`)
- avoid forcing non-interactive/browser defaults from `--target`
- preserve standalone target behavior by skipping skills integration in full setup
- improve setup help text with agent-friendly non-interactive examples

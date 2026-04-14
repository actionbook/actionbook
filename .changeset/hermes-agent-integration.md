---
"@actionbookdev/cli": patch
---

Add Hermes agent integration.

- New `SetupTarget::Hermes` variant invokes `hermes skills install` directly instead of routing through `npx skills add`, targeting Hermes's native skill registry at `~/.hermes/skills/`
- Missing-binary error now points users to install Hermes (not Node.js) when the target is Hermes
- Post-install verification parses `hermes skills list` table columns exactly (by `│` delimiter) to avoid false positives from similarly-named skills
- `skills/actionbook/SKILL.md` gains Hermes-compatible frontmatter (`metadata.hermes.tags`, `requires_toolsets: [terminal]`, `prerequisites.commands: [actionbook]`) so the skill is discoverable via `hermes skills search` and hidden on non-terminal platforms

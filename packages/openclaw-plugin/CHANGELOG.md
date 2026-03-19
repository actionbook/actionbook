# @actionbookdev/openclaw-plugin

## 0.1.3

### Patch Changes

- [#228](https://github.com/actionbook/actionbook/pull/228) [`b825041`](https://github.com/actionbook/actionbook/commit/b82504129da9a00f8a44d750665a68307502f788) Thanks [@Senke0x](https://github.com/Senke0x)! - Remove unconditional system prompt injection via before_prompt_build hook. Agent guidance is now provided exclusively through the bundled SKILL.md, which OpenClaw activates on demand based on user intent.

## 0.1.2

### Patch Changes

- [#222](https://github.com/actionbook/actionbook/pull/222) [`4677a4c`](https://github.com/actionbook/actionbook/commit/4677a4c70f53d2c0f0a512fe48cbe26b334ec65f) Thanks [@Senke0x](https://github.com/Senke0x)! - fix: resolve workspace:\* protocol in npm publish by switching to pnpm publish

## 0.1.1

### Patch Changes

- [#213](https://github.com/actionbook/actionbook/pull/213) [`26213c9`](https://github.com/actionbook/actionbook/commit/26213c973beb60dddc8fa490aaa5875fc81fc09e) Thanks [@Senke0x](https://github.com/Senke0x)! - Initial release of Actionbook OpenClaw plugin with search_actions and get_action_by_area_id tools.

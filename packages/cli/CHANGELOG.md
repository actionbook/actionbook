# @actionbookdev/cli

## 0.7.5

### Patch Changes

- [#159](https://github.com/actionbook/actionbook/pull/159) [`6ad3b57`](https://github.com/actionbook/actionbook/commit/6ad3b5708af1b16548c61e9f60121f72368229e5) Thanks [@Senke0x](https://github.com/Senke0x)! - Refine `actionbook setup` behavior for agent and non-interactive workflows:

  - remove `--agent-mode` and keep setup targeting via `--target`
  - keep `--target` quick mode only when used alone
  - run full setup when `--target` is combined with setup flags (for example `--non-interactive`, `--browser`, `--api-key`)
  - avoid forcing non-interactive/browser defaults from `--target`
  - preserve standalone target behavior by skipping skills integration in full setup
  - improve setup help text with agent-friendly non-interactive examples

## 0.7.4

### Patch Changes

- [#153](https://github.com/actionbook/actionbook/pull/153) [`defe7f8`](https://github.com/actionbook/actionbook/commit/defe7f88ff401ba1bf6c2043479039d37dc0d255) Thanks [@adcentury](https://github.com/adcentury)! - Add a simple welcome screen to `actionbook setup` showing the Actionbook logo and name.

## 0.7.3

### Patch Changes

- [#135](https://github.com/actionbook/actionbook/pull/135) [`deedfe8`](https://github.com/actionbook/actionbook/commit/deedfe8836c56ac3b48123989405afd84a06bad7) Thanks [@4bmis](https://github.com/4bmis)! - Use changesets to manage packages

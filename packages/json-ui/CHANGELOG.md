# @actionbookdev/json-ui

## 0.3.0

### Minor Changes

- [#164](https://github.com/actionbook/actionbook/pull/164) [`8f85d69`](https://github.com/actionbook/actionbook/commit/8f85d693d5b7d61118bc9b5805aadc0190643ba4) Thanks [@ZhangHanDong](https://github.com/ZhangHanDong)! - Add syntax highlighting, code wrapping, and language switcher control

  - Add `showLanguageSwitcher` prop to Report component for controlling language toggle visibility
  - Fix markdown code block rendering to support ```lang syntax
  - Integrate Prism.js for syntax highlighting with support for 9 languages (Rust, JavaScript, TypeScript, Python, Bash, JSON, YAML, TOML, Markdown)
  - Add automatic theme switching for syntax highlighting (light/dark modes)
  - Enable code wrapping with `white-space: pre-wrap` for better mobile readability
  - Add enhanced CSS styling for code blocks with proper borders, backgrounds, and spacing
  - Add SRI (Subresource Integrity) hashes to all Prism.js CDN resources for supply chain security

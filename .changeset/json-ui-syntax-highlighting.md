---
"@actionbookdev/json-ui": minor
---

Add syntax highlighting, code wrapping, and language switcher control

- Add `showLanguageSwitcher` prop to Report component for controlling language toggle visibility
- Fix markdown code block rendering to support ```lang syntax
- Integrate Prism.js for syntax highlighting with support for 9 languages (Rust, JavaScript, TypeScript, Python, Bash, JSON, YAML, TOML, Markdown)
- Add automatic theme switching for syntax highlighting (light/dark modes)
- Enable code wrapping with `white-space: pre-wrap` for better mobile readability
- Add enhanced CSS styling for code blocks with proper borders, backgrounds, and spacing

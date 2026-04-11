---
"@actionbookdev/cli": minor
---

Improve `actionbook setup` with a new skills installation step and targeted quick mode.

- add a fifth setup step to install Actionbook skills during setup
- add `actionbook setup --target <agent>` quick mode for one-shot skills installation
- improve extension-mode setup guidance with Chrome Web Store and GitHub Releases fallback instructions
- make API key input visible by default during interactive setup
- tighten setup failure handling so quick mode and JSON flows report skills install failures correctly

---
"@actionbookdev/cli": minor
---

`browser eval` now accepts expression input from a file via `--file <path>` or from stdin when the positional expression is omitted. Existing positional-arg usage is unchanged. Useful for evaluating multi-line scripts without escaping them on the shell.

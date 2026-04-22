---
"@actionbookdev/cli": minor
---

HAR truncation now surfaces explicitly in the `browser network har stop` envelope. When the FIFO ring buffer dropped older entries, the response adds `meta.truncated: true`, `meta.warnings: ["HAR_TRUNCATED: N earlier entries dropped (max_entries=M); raise --max-entries or stop recording sooner to keep the full trace"]`, and `data.max_entries` (the configured cap). Clean stops emit no truncation marker. `DEFAULT_MAX_ENTRIES` is bumped from 2000 to 10000 so longer interactive sessions fit without truncation.

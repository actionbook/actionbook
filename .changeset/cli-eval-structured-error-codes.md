---
"@actionbookdev/cli": minor
---

`browser eval` now returns a structured error envelope with an `EvalErrorCode` taxonomy in `error.code`: `EVAL_COMPILE_ERROR`, `EVAL_RUNTIME_ERROR`, `EVAL_TIMEOUT`, `EVAL_SERIALIZATION_ERROR`, `EVAL_INVALID_INPUT`. The raw V8 reason and line/column offsets are preserved under `error.details`. Replaces the previous free-form error message so callers can branch on the code instead of string-matching.

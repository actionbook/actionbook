---
"@actionbookdev/cli": minor
---

`browser eval` returns structured error codes (`EVAL_COMPILE_ERROR`, `EVAL_RUNTIME_ERROR`, `EVAL_TIMEOUT`, `EVAL_SERIALIZATION_ERROR`, `EVAL_INVALID_INPUT`) with the raw V8 reason preserved in `error.details`.

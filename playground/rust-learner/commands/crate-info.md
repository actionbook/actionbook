# /rust-learner:crate-info

Get information about a Rust crate including latest version, features, and changelog.

## Usage

```
/rust-learner:crate-info <crate> [version]
```

## Parameters

- `crate` (required): Crate name (e.g., `tokio`, `serde`, `axum`)
- `version` (optional): Specific version to look up

## Examples

```
/rust-learner:crate-info tokio           # Latest tokio info
/rust-learner:crate-info axum 0.7        # axum 0.7 features
/rust-learner:crate-info serde           # serde latest features
```

## Workflow

1. Use `search_actions("lib.rs crate")` to get action ID
2. Use `get_action_by_id()` to get page selectors
3. Use `agent-browser` to open https://lib.rs/crates/{crate}
4. Extract crate information and changelog
5. Summarize for user

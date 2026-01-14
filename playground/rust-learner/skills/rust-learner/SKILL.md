---
name: rust-learner
description: Learn Rust language features and crate updates. Use when user asks about Rust version changelog, what's new in Rust, crate updates, Cargo.toml dependencies, tokio/serde/axum features, or any Rust ecosystem questions.
---

# Rust Learner

Learn Rust by fetching real-time information about Rust language features and crate updates.

## ⚠️ Tool Priority & Waiting Rule

**Priority:**
1. ✅ `browser-fetcher` agent (preferred)
2. ⚠️ `Fetch` / `WebFetch` (only after all browser-fetcher agents fail)
3. ⚠️ `WebSearch` (only when search engine results are needed)

**⛔ No "Racing Ahead":**
- After launching browser-fetcher agents, **MUST wait for ALL of them to complete**
- **DO NOT** use WebSearch/Fetch as "supplements" while waiting
- Only use fallback tools after **ALL** browser-fetcher agents have failed

## Workflow

### Step 1: actionbook MCP

```
search_actions("lib.rs crate")  → get action ID
get_action_by_id(id)            → get URL and selectors
```

### Step 2: Launch browser-fetcher agents

```
Launch multiple browser-fetcher agents in parallel
```

### Step 3: Wait for ALL agents to complete

```
⛔ DO NOT use other tools during this time
✅ Wait for TaskOutput to return all results
```

### Step 4: Summarize results

- If agents succeed: summarize content for user
- If ALL agents fail: use Fetch as fallback

## Example

```
User: Query tokio latest version

✅ CORRECT:
1. Launch browser-fetcher: lib.rs/crates/tokio
2. Launch browser-fetcher: crates.io/crates/tokio
3. Wait for BOTH agents to complete
4. Summarize results

❌ WRONG:
1. Launch browser-fetcher agents
2. While waiting, use WebSearch("tokio latest")  ← Racing ahead!
3. Mix multiple result sources
```

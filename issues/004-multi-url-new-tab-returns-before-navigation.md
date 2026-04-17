# Issue #004: `browser new-tab` with multiple URLs returns before navigation starts

**Date:** 2026-04-18
**Severity:** **High** — breaks the parallel-tab research pattern
**Status:** Open
**Affected component:** `packages/cli/src/browser/navigation/new_tab.rs` (or equivalent multi-URL handler)
**Discovered during:** L4 verification run with the newly-landed "Parallel sources" SKILL.md pattern

## Symptom

When `browser new-tab` is called with N URLs and `--tab t1 --tab t2 ... --tab tN` to create N tabs concurrently, the command returns with `opened_tabs: N, failed_urls: 0` **before** any of the tabs have actually navigated to their target URLs. The tabs exist but are still at `about:blank`.

Any immediately-following `browser wait network-idle` call on those tabs returns in ~0 ms (because `about:blank` has no network activity), and the subsequent `browser text` reads an empty document. The full `new-tab` + parallel `wait + text` block completes in ~600 ms (way too fast for three real article page loads), returning 0 bytes of content on all tabs.

Critically: within ~1 s the tabs DO navigate correctly on their own. A subsequent `list-tabs` or `text` call picks up the real URL and returns real content.

## Reproduction

```bash
AB=/Users/zhangalex/Work/Projects/actionbook/actionbook/packages/cli/target/release/actionbook

$AB browser close --session repro 2>/dev/null || true
$AB browser start --session repro

# Step A: multi-URL new-tab
$AB browser new-tab \
  "https://www.techtarget.com/" \
  "https://obeli.sk/" \
  "https://component-model.bytecodealliance.org/" \
  --session repro --tab a --tab b --tab c

# Step B: immediately parallel wait + text --readable
(
  $AB browser wait network-idle --session repro --tab a --timeout 15000 > /dev/null
  $AB browser text --readable --session repro --tab a --json | jq '{url: .context.url, size: (.data.value | length), ms: .meta.duration_ms}'
) &
(
  $AB browser wait network-idle --session repro --tab b --timeout 15000 > /dev/null
  $AB browser text --readable --session repro --tab b --json | jq '{url: .context.url, size: (.data.value | length), ms: .meta.duration_ms}'
) &
(
  $AB browser wait network-idle --session repro --tab c --timeout 15000 > /dev/null
  $AB browser text --readable --session repro --tab c --json | jq '{url: .context.url, size: (.data.value | length), ms: .meta.duration_ms}'
) &
wait
```

**Expected:** 3 results with real URLs and ≥ 500-byte content each.
**Observed:** 3 results with `url: "about:blank", size: 0, ms: 3`.

Waiting 1 second between Step A and Step B makes the test pass. A subsequent (serial) `text --readable` on the same tabs also passes because by that point navigation has completed.

## Root cause hypothesis

The `new-tab` handler kicks off navigation via `Page.navigate` (or equivalent CDP) but does not await any of:

- `Page.frameStartedLoading` — would confirm navigation started
- `Page.frameNavigated` — would confirm URL changed from `about:blank`

So the command returns to the CLI as soon as the tabs are created, not when they've transitioned away from `about:blank`. The `.data.tabs[*].url` field in the response does reflect the **requested** URL (that's the input), but `.data.tabs[*].native_tab_id` maps to a tab that is still at `about:blank` inside CDP.

The single-URL case was not affected in testing — likely because the synchronous path had enough latency for navigation to start, or the handler happened to await the first frame event. Multi-URL fires the `navigate` commands in parallel without awaiting any of them.

## Why tests did not catch this

- The existing E2E suite uses the in-process harness fixture server, where request latency is microseconds. `about:blank` → fixture URL transition happens fast enough that the race never manifests.
- Single-URL `new-tab` may have a different code path or different await semantics.
- The 2026-04-17 `--readable` live A/B also tested only a single URL at a time.

## Workarounds

**Per call:** After `new-tab` with multiple URLs, poll `list-tabs` until all target tabs' `.url` field no longer equals `about:blank`:

```bash
# Naive workaround — busy-wait, costs round-trips
until $AB browser list-tabs --session s --json | jq -e '[.data.tabs[] | .url | startswith("about:")] | any | not' > /dev/null; do
  sleep 0.2
done
```

**Pragmatic:** Use serial reads for the moment — the parallel-browser pattern in SKILL.md should be downgraded until this is fixed, or the pattern amended to do a 1 s sleep after multi-URL new-tab.

## Remediation plan

1. Inside `new-tab` handler, when multiple URLs are supplied:
   - Fire `navigate` for each tab
   - `await`/`join` all `frameStartedLoading` events (or an equivalent CDP confirmation) before returning
2. Add E2E scenario `new_tab_multi_url_returns_after_navigation_starts` that:
   - Creates 3 tabs pointing at `<harness>/slow` (intentionally slow-loading fixture)
   - Immediately reads `list-tabs` and asserts no tab is at `about:blank`
3. Update SKILL.md once fixed to restore the fully-parallel Pattern 2 without the workaround.

## Discovered during

L4 verification of research-api-adapter SKILL.md changes (2026-04-18). Topic: "WebAssembly Component Model 2026 status". The content smell test correctly caught all three empty responses, preventing silent synthesis from zero-byte sources. Research fell back to serial reads and completed successfully. Report at `output/wasm-component-model-2026.html`.

# Actionbook CLI — iframe Support Design Spec

> **Date:** 2026-03-31
> **Status:** Draft
> **Scope:** `packages/cli-v2`

## Overview

Add iframe content expansion to the CLI v2 snapshot system. Currently, snapshots only capture the main frame's accessibility tree, ignoring content inside iframes (embedded forms, OAuth popups, third-party widgets, etc.).

Goals:
- Automatically expand 1 level of iframe content in snapshots
- Track cross-origin iframe CDP sessions
- Support `@eN` ref-based interactions with elements inside iframes

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| CLI interface changes | No new commands or flags | Agents should not be aware of iframe boundaries; transparent integration |
| iframe expansion depth | 1 level only | Prevents unbounded recursion; sufficient for most real-world use cases |
| Cross-origin iframe failure | Silent skip | Inaccessible iframes must not block the entire snapshot |
| iframe session storage | Inside CdpSession | Same level as tab_sessions; event-driven population via reader_loop |
| Domain enable timing | Deferred drain (at snapshot time) | Avoids re-entrancy issues from sending CDP commands inside reader_loop |
| RefEntry extension | New frame_id field | RefCache is daemon-internal, not serialized to wire; no compatibility concerns |
| RefCache key collision | Composite key `(frame_id, backendNodeId)` | Different frames may reuse the same backendNodeId; bare i64 key would cause overwrites (see existing TODO at snapshot_transform.rs:560) |
| Element interaction routing | backendNodeId + CDP session routing | Reuses CDP coordinate system; avoids JS injection into iframes |

## User-Facing Usage

### Snapshot — Automatic iframe Content Expansion

```bash
# Usage unchanged; iframe content appears automatically
actionbook browser snapshot --session s1 --tab t1
```

Output (iframe content indented under the Iframe node):

```
- navigation "Main Nav" [ref=e1]
- heading "Welcome" [ref=e2]
- Iframe "" [ref=e3]
    - textbox "Email" [ref=e4]
    - textbox "Password" [ref=e5]
    - button "Sign In" [ref=e6]
- paragraph "Footer text"
```

All existing flags work as before: `-i` (interactive), `-c` (compact), `--cursor`, `--selector`, `--depth`.

### Interaction — Refs Auto-Route to iframe

```bash
# Refs inside iframes work identically to main-page refs
actionbook browser click @e6 --session s1 --tab t1
actionbook browser fill @e4 "user@example.com" --session s1 --tab t1
actionbook browser type "mypassword" --session s1 --tab t1
```

### Behavior Rules

| Scenario | Behavior |
|----------|----------|
| Same-origin iframe | Content expanded, refs interactive |
| Cross-origin iframe (accessible) | Content expanded, refs interactive |
| Cross-origin iframe (inaccessible) | Silently skipped (Iframe node present but no children, no error) |
| Nested iframe (iframe within iframe) | Only 1st level expanded; deeper levels not expanded |
| Agent awareness requirement | None — agents do not need to know whether an element is inside an iframe |

## Architecture

```
+--------------------------------------------------------------------+
|                         Daemon (server.rs)                          |
|                                                                     |
|  +------------------------------------------------------------+    |
|  |                    CdpSession                               |    |
|  |                                                             |    |
|  |  tab_sessions:    { target_id -> cdp_session_id }           |    |
|  |  iframe_sessions: { frame_id  -> cdp_session_id }  <- NEW  |    |
|  |                                                             |    |
|  |  reader_loop:                                               |    |
|  |    Response (id match) -> route to pending caller           |    |
|  |    Target.attachedToTarget (type=iframe) -> iframe_sessions |    |
|  |    Target.detachedFromTarget -> cleanup iframe_sessions     |    |
|  |    Network.* -> tab_net_pending counter                     |    |
|  +------------------------------------------------------------+    |
|                                                                     |
|  +--------------+  +--------------+  +----------------------+       |
|  |  Registry    |  |  RefCache    |  |  Router              |       |
|  |              |  |              |  |                      |       |
|  |  sessions    |  |  id_to_ref:  |  |  Action::Snapshot    |       |
|  |  tabs        |  |  (frame_id,  |  |    -> snapshot.rs    |       |
|  |  ref_caches  |  |   bid) ->    |  |    -> expand iframes |       |
|  |              |  |    RefEntry{ |  |                      |       |
|  |              |  |      ref_id  |  |  Action::Click(@eN)  |       |
|  |              |  |      role    |  |    -> element.rs     |       |
|  |              |  |      name    |  |    -> route to iframe|       |
|  |              |  |      frame_id|  |      CDP session     |       |
|  |              |  |    }         |  |                      |       |
|  +--------------+  +--------------+  +----------------------+       |
+--------------------------------------------------------------------+
```

### iframe Snapshot Expansion Flow

```
snapshot execute()
    |
    v
Accessibility.getFullAXTree (main frame)
    |
    v
parse_ax_tree() -> nodes[] (includes role="Iframe" nodes)
    |
    v
drain_pending_iframe_enables()
    | -> DOM.enable + Accessibility.enable on each iframe session
    v
for each Iframe node with backend_node_id:
    |
    +- DOM.describeNode(backendNodeId, depth:1)
    |   -> contentDocument.frameId -> child_frame_id
    |
    +- iframe_sessions.get(child_frame_id)?
    |   +- Found (cross-origin) -> Accessibility.getFullAXTree on iframe_session
    |   +- Not found (same-origin) -> Accessibility.getFullAXTree with frameId param
    |
    +- parse_ax_tree(child_response, frame_id=child_frame_id)
    |   -> child refs get frame_id recorded in RefCache
    |
    +- render_content(child_nodes) -> indent -> insert after Iframe line
```

### Ref Interaction Routing Flow (Session Propagation)

```
ctx.resolve_center("@e5")
    |
    v
resolve_node("@e5")
    |
    v
RefCache lookup -> backendNodeId=42, frame_id=Some("FRAME_ABC")
    |
    v
execute_for_frame(cdp, target_id, Some("FRAME_ABC"), "DOM.resolveNode", {backendNodeId: 42})
    |
    +- iframe_sessions.get("FRAME_ABC")?
    |   +- Found (cross-origin) -> cdp.execute("DOM.resolveNode", ..., Some(iframe_sid))
    |   +- Not found (same-origin) -> cdp.execute_on_tab(target_id, "DOM.resolveNode", ...)
    |
    +- -> ResolvedNode { node_id: 789, frame_id: Some("FRAME_ABC") }
         |
         v
    scroll_into_view(cdp, target_id, 789, Some("FRAME_ABC"))  <- uses execute_for_frame
         |
         v
    get_element_center(cdp, target_id, 789, ..., Some("FRAME_ABC"))  <- uses execute_for_frame
         |
         v
    (x, y) viewport coordinates
         |
         v
    dispatch_click(cdp, target_id, x, y, ...)  <- uses execute_on_tab (page-level, no frame routing)
```

## Implementation Phases

### Phase 1: CDP iframe Session Tracking

**File:** `src/daemon/cdp_session.rs`

1. Add `iframe_sessions: Arc<Mutex<HashMap<String, String>>>` field
2. Add `pending_iframe_enables: Arc<Mutex<Vec<String>>>` field
3. Handle `Target.attachedToTarget` (type=iframe) and `Target.detachedFromTarget` events in `reader_loop`
4. Call `Target.setAutoAttach` in `attach()` after the existing `Network.enable`

```rust
// Added to attach():
let _ = self.execute("Target.setAutoAttach", json!({
    "autoAttach": true,
    "waitForDebuggerOnStart": false,
    "flatten": true
}), Some(&session_id)).await;
```

5. New public methods:
   - `iframe_sessions(&self) -> HashMap<String, String>` — clone the iframe sessions map
   - `drain_pending_iframe_enables(&self) -> Vec<String>` — take pending enables
   - `clear_iframe_sessions(&self)` — clear all (used by session close/restart)

### Phase 2: RefCache — Frame-Aware Keying

**File:** `src/browser/observation/snapshot_transform.rs`

**Problem:** The current `id_to_ref: HashMap<i64, RefEntry>` is keyed by bare `backendNodeId`. Different frames (main + iframe) may reuse the same backendNodeId values, causing overwrites. The existing TODO at line 560 explicitly flags this.

**Solution:** Change the RefCache primary key from `i64` to `(Option<String>, i64)` — a composite of `(frame_id, backendNodeId)`. This resolves the collision without splitting into per-frame caches (which would complicate the single-tab ref namespace that agents see).

1. Add `frame_id: Option<String>` field to `RefEntry`
2. Change `id_to_ref` key from `i64` to a composite type:
   ```rust
   /// Composite key: (frame_id, backendNodeId).
   /// Main frame uses None; iframe elements use Some(frame_id).
   type RefKey = (Option<String>, i64);

   id_to_ref: HashMap<RefKey, RefEntry>,
   ref_to_id: HashMap<String, RefKey>,    // ref_id -> (frame_id, backendNodeId)
   ```
3. Update `get_or_assign()` signature to accept `frame_id: Option<&str>`:
   ```rust
   pub fn get_or_assign(&mut self, backend_node_id: i64, role: &str, name: &str, frame_id: Option<&str>) -> String
   ```
4. Add `frame_id_for_ref(&self, ref_id: &str) -> Option<&str>` — extracts frame_id from the stored RefKey
5. Add `backend_node_id_for_ref()` — updated to return the `i64` component of the RefKey
6. Update all existing callers in `parse_ax_tree` to pass `frame_id: None` (main frame)
7. Add `frame_id: Option<&str>` parameter to `parse_ax_tree()`, passed through to `ref_cache.get_or_assign()`

### Phase 3: Snapshot iframe Expansion

**File:** `src/browser/observation/snapshot.rs`

1. Add `resolve_iframe_frame_id()` — `DOM.describeNode` -> `contentDocument.frameId`
2. Add `fetch_iframe_ax_tree()` — cross-origin uses iframe session; same-origin uses parent session + frameId
3. In `execute()`: after main frame parse, iterate Iframe nodes -> expand -> indent and insert

### Phase 4: iframe-Aware Session Propagation

The core requirement: **all CDP operations on an iframe element must use the correct CDP session** — not just the initial resolution, but every subsequent DOM query, focus, JS evaluation, etc.

#### Key Insight: Input Events vs DOM/Runtime Queries

CDP `Input.dispatchMouseEvent` operates at the **page level** with viewport coordinates — Chrome routes the event to the correct frame automatically. So `dispatch_click`, `dispatch_key_event`, etc. do NOT need frame routing.

The commands that DO need frame-aware routing are **DOM/Runtime queries** on cross-origin iframe elements:
- `DOM.resolveNode`, `DOM.focus`, `DOM.scrollIntoViewIfNeeded`, `DOM.getBoxModel`
- `Runtime.callFunctionOn`, `Runtime.evaluate` (when targeting iframe context)
- `Accessibility.queryAXTree`

For **same-origin** iframes, these work on the parent session (shared DOM). For **cross-origin** (OOPIF) iframes, a dedicated CDP session is required.

#### The Problem: Post-Resolution Commands

Many interaction/observation commands follow this pattern:
```
1. ctx.resolve_node(selector)  →  node_id        (TabContext method)
2. ctx.resolve_object_id(node_id)  →  object_id  (TabContext method)
3. ctx.cdp.execute_on_tab(target_id, "Runtime.callFunctionOn", {objectId, ...})  ← DIRECT CDP call
4. ctx.cdp.execute_on_tab(target_id, "DOM.focus", {nodeId})                      ← DIRECT CDP call
```

Steps 1-2 can be updated inside TabContext. But step 3-4 are **direct CDP calls in each command file**, bypassing TabContext. These are found in:
- `focus.rs` — `DOM.focus`, `Runtime.evaluate` (pre/post focus comparison)
- `hover.rs` — `Runtime.callFunctionOn` (hover event dispatch)
- `fill.rs` — `DOM.focus`, `Runtime.evaluate` (set value via native setter)
- `select.rs` — `Runtime.callFunctionOn` (option selection)
- `click.rs` — `Runtime.callFunctionOn` (href extraction)
- `html.rs`, `text.rs`, `value.rs` — `Runtime.callFunctionOn` (read element properties)

#### Solution: TabContext Frame-Aware Execution Methods

Instead of changing every command file, add **frame-aware execution methods on TabContext** that replace `ctx.cdp.execute_on_tab()`:

**File:** `src/browser/element.rs`

**4.1 Add `execute_for_frame` as a standalone helper:**

```rust
/// Execute a CDP command on the correct session for a given frame_id.
/// Cross-origin iframes (in cdp.iframe_sessions()) use their dedicated session.
/// Same-origin iframes and main frame use execute_on_tab (parent session).
pub async fn execute_for_frame(
    cdp: &CdpSession,
    target_id: &str,
    frame_id: Option<&str>,
    method: &str,
    params: Value,
) -> Result<Value, CliError>
```

**4.2 Add `ResolvedNode` to carry frame context through the chain:**

```rust
pub struct ResolvedNode {
    pub node_id: i64,
    pub frame_id: Option<String>,
}
```

**4.3 Update TabContext to track and propagate frame context:**

```rust
pub struct TabContext {
    pub cdp: CdpSession,
    pub target_id: String,
    registry: SharedRegistry,
    session_id: String,
    tab_id: String,
    /// Frame context set by the most recent resolve_node / resolve_center / resolve_object call.
    /// Used by execute_in_frame() for subsequent CDP commands on the same element.
    resolved_frame_id: Option<String>,
}
```

**Signature change: `&self` → `&mut self`**: `resolve_node`, `resolve_center`, `resolve_object` all change from `&self` to `&mut self` because they write `resolved_frame_id`. This propagates to all command files that hold a `TabContext` — they must declare `let mut ctx = ...` instead of `let ctx = ...`. This is a mechanical change (no logic change) but affects every command file that uses TabContext.

Updated methods on TabContext (all resolution methods set `resolved_frame_id`):
```rust
/// Selector → nodeId. Sets resolved_frame_id for @eN refs (None for CSS/XPath).
/// All subsequent execute_in_frame() calls use this frame context.
pub async fn resolve_node(&mut self, selector: &str) -> Result<i64, ActionResult>

/// Selector → centre (x, y). Calls resolve_node internally, inherits frame context.
pub async fn resolve_center(&mut self, selector: &str) -> Result<(f64, f64), ActionResult>

/// Selector → (nodeId, objectId). Calls resolve_node internally, inherits frame context.
pub async fn resolve_object(&mut self, selector: &str) -> Result<(i64, String), ActionResult>

/// Execute a CDP command on the frame of the most recently resolved element.
/// Commands that need this: DOM.focus, Runtime.callFunctionOn, Runtime.evaluate (on element).
/// Falls back to execute_on_tab if no frame context is set (main frame / CSS / XPath).
pub async fn execute_in_frame(&self, method: &str, params: Value) -> Result<Value, CliError>
```

**Key invariant**: every resolution method (`resolve_node`, `resolve_center`, `resolve_object`) sets `resolved_frame_id`. There is no separate `resolve_node_with_frame()` — a single API handles both main-frame and iframe selectors. Commands that call `resolve_node()` followed by `execute_in_frame()` will always have the correct frame context.

**4.4 Interaction command changes — two categories:**

**Category A: Commands that only use page-level input dispatch (no direct DOM/Runtime calls after resolve)**
- `drag.rs` — uses TabContext; calls `resolve_center` twice. Needs `let mut ctx` due to `&mut self` signature change. No frame-aware CDP calls needed (subsequent ops are `Input.dispatchMouseEvent`).
- `press.rs` — does not use TabContext (dispatches `Input.dispatchKeyEvent` directly). No changes needed.
- `cursor_position.rs` — does not use TabContext (reads stored position). No changes needed.
- `mouse_move.rs` — does not use TabContext (uses `get_cdp_and_target` + `Input.dispatchMouseEvent`). No changes needed.

**Category B: Commands that make direct `ctx.cdp.execute_on_tab()` calls after resolve**
These must change `ctx.cdp.execute_on_tab(&ctx.target_id, ...)` → `ctx.execute_in_frame(...)`:

| File | CDP calls that need frame routing |
|------|-----------------------------------|
| `focus.rs` | `DOM.focus({ nodeId })`, `Runtime.evaluate` (pre/post focus) |
| `hover.rs` | `Runtime.callFunctionOn({ objectId, ... })` |
| `fill.rs` | `DOM.focus({ nodeId })`, `Runtime.evaluate` (value setter) |
| `select.rs` | `Runtime.callFunctionOn({ objectId, ... })` |
| `click.rs` | `Runtime.callFunctionOn({ objectId, ... })` in `get_element_href` only |
| `type_text.rs` | `DOM.focus({ nodeId })`, `Runtime.evaluate` (selection range) |
| `scroll.rs` | `Runtime.callFunctionOn({ objectId, ... })` on container/element |
| `upload.rs` | `DOM.setFileInputFiles({ nodeId })` |
| `html.rs` | `Runtime.callFunctionOn({ objectId, ... })` |
| `text.rs` | `Runtime.callFunctionOn({ objectId, ... })` |
| `value.rs` | `Runtime.callFunctionOn({ objectId, ... })` |

NOTE: `eval.rs` is excluded — it runs arbitrary JS on the main frame context (not element-scoped), so it stays on the parent session. A future `--frame` flag could add iframe targeting.

**4.5 Observation command changes:**

| File | Changes |
|------|---------|
| `screenshot.rs` | `collect_annotation_rects()` — use `execute_for_frame` for `DOM.resolveNode` per RefEntry's frame_id. Silently skip detached iframe refs. |
| `attr.rs`, `attrs.rs`, `styles.rs`, `box.rs`, `state.rs`, `describe.rs` | Same pattern as html.rs — `Runtime.callFunctionOn` after resolve_object. Use `ctx.execute_in_frame()`. |

**4.6 Implementation order within Phase 4:**

1. Add `execute_for_frame()` standalone helper
2. Add `ResolvedNode` struct
3. Update `resolve_ref` / `resolve_backend_node` / `resolve_by_ax_query` to use `execute_for_frame`
4. Add `resolved_frame_id` to TabContext + `execute_in_frame()` method
5. Update `resolve_center` / `resolve_object` to set `resolved_frame_id`
6. Update Category B command files: replace `ctx.cdp.execute_on_tab(&ctx.target_id, ...)` with `ctx.execute_in_frame(...)`
7. Update `collect_annotation_rects` in screenshot.rs

**File:** `src/browser/observation/inspect_point.rs`

Uses `get_or_assign(bid, role, name, None)` — already updated in Phase 2. No frame routing needed here since inspect-point operates on viewport coordinates in the main frame.

### Phase 5: Cleanup

**Files:** `src/browser/session/close.rs`, `src/browser/session/restart.rs`

1. Call `cdp.clear_iframe_sessions()` as a safety net
2. Navigation cleanup is already handled (`registry.clear_ref_cache`); no additional changes needed

## Files to Modify

| File | Changes |
|------|---------|
| `src/daemon/cdp_session.rs` | iframe_sessions map, reader_loop event handling, Target.setAutoAttach, public accessors |
| `src/browser/observation/snapshot_transform.rs` | RefEntry.frame_id, composite RefKey, get_or_assign() signature, frame_id_for_ref(), parse_ax_tree() frame_id param |
| `src/browser/observation/snapshot.rs` | resolve_iframe_frame_id(), fetch_iframe_ax_tree(), iframe expansion in execute() |
| `src/browser/element.rs` | execute_for_frame() helper, ResolvedNode struct, TabContext.resolved_frame_id + execute_in_frame(), resolve_ref/resolve_backend_node/resolve_by_ax_query frame routing, scroll_into_view/resolve_object_id/get_element_center frame_id param |
| `src/browser/interaction/focus.rs` | DOM.focus + Runtime.evaluate → ctx.execute_in_frame() |
| `src/browser/interaction/hover.rs` | Runtime.callFunctionOn → ctx.execute_in_frame() |
| `src/browser/interaction/fill.rs` | DOM.focus + Runtime.evaluate → ctx.execute_in_frame() |
| `src/browser/interaction/select.rs` | Runtime.callFunctionOn → ctx.execute_in_frame() |
| `src/browser/interaction/click.rs` | Runtime.callFunctionOn in get_element_href → ctx.execute_in_frame() |
| `src/browser/interaction/type_text.rs` | DOM.focus + Runtime.evaluate (selection range) → ctx.execute_in_frame() |
| `src/browser/interaction/scroll.rs` | Runtime.callFunctionOn on container/element objects → execute_for_frame via helper fns |
| `src/browser/interaction/upload.rs` | DOM.setFileInputFiles({ nodeId }) → ctx.execute_in_frame() |
| `src/browser/interaction/drag.rs` | `execute_inner(&ctx, ...)` → `execute_inner(&mut ctx, ...)`: fn signature change propagates through execute() → execute_inner() → two resolve_center() calls |
| `src/browser/observation/html.rs` | Runtime.callFunctionOn → ctx.execute_in_frame() |
| `src/browser/observation/text.rs` | Runtime.callFunctionOn → ctx.execute_in_frame() |
| `src/browser/observation/value.rs` | Runtime.callFunctionOn → ctx.execute_in_frame() |
| `src/browser/observation/attr.rs` | Runtime.callFunctionOn → ctx.execute_in_frame() |
| `src/browser/observation/attrs.rs` | Runtime.callFunctionOn → ctx.execute_in_frame() |
| `src/browser/observation/styles.rs` | Runtime.callFunctionOn → ctx.execute_in_frame() |
| `src/browser/observation/box.rs` | Runtime.callFunctionOn → ctx.execute_in_frame() |
| `src/browser/observation/state.rs` | Runtime.callFunctionOn → ctx.execute_in_frame() |
| `src/browser/observation/describe.rs` | Runtime.callFunctionOn → ctx.execute_in_frame() |
| `src/browser/observation/screenshot.rs` | collect_annotation_rects() frame-aware DOM.resolveNode routing |
| `src/browser/observation/inspect_point.rs` | get_or_assign() signature updated (frame_id=None) |
| `src/browser/session/close.rs` | clear_iframe_sessions() call |
| `src/browser/session/restart.rs` | clear_iframe_sessions() call |

### Files NOT Modified

- CLI layer (`cli.rs`, `main.rs`) — no new commands or flags
- Wire protocol (`action.rs`, `action_result.rs`) — no new Action variants
- Output formatting (`output.rs`) — snapshot output format unchanged
- Router (`daemon/router.rs`) — no new routes
- `press.rs`, `cursor_position.rs`, `mouse_move.rs` — only Input.dispatch* (page-level) or stored data, no TabContext usage
- `eval.rs` — runs arbitrary JS on main frame context (not element-scoped)

## Verification

### Unit Tests
1. Mock CDP responses, verify `resolve_iframe_frame_id` correctly extracts frameId
2. RefCache composite key — two different frames with same backendNodeId must receive distinct ref IDs (e.g., main frame bid=42 -> e1, iframe bid=42 -> e2)
3. RefCache frame_id round-trip — `get_or_assign` with frame_id, then `frame_id_for_ref` returns correct value
4. `execute_for_frame` routing — mock iframe_sessions, verify cross-origin routes to iframe session, same-origin routes to parent

### E2E Tests — Snapshot
5. Navigate to page with same-origin iframe -> snapshot -> verify iframe content appears indented under Iframe node
6. `--interactive` flag -> verify iframe interactive elements receive refs

### E2E Tests — Interaction (coordinate-based)
7. snapshot -> click `@eN` ref inside iframe -> verify click dispatched correctly

### E2E Tests — Interaction (object-based, validates session propagation)
8. snapshot -> `html @eN` on iframe element -> verify innerHTML returned (Runtime.callFunctionOn path)
9. snapshot -> `fill @eN "text"` on iframe input -> verify value set (DOM.focus + Runtime.evaluate path)
10. snapshot -> `hover @eN` on iframe element -> verify no error (Runtime.callFunctionOn path)
11. snapshot -> `scroll into-view @eN` on iframe element -> verify no error (Runtime.callFunctionOn on resolved object)
12. snapshot -> `upload @eN /tmp/test.txt` on iframe file input -> verify no error (DOM.setFileInputFiles with frame-routed nodeId)

### E2E Tests — Observation
13. screenshot `--annotate` after snapshot with iframe content -> verify iframe element annotations render

### Manual Tests
14. Cross-origin iframe (e.g., embedded YouTube) -> verify silent skip in snapshot (no error)
15. Cross-origin accessible iframe (e.g., Stripe checkout) -> verify content expanded and refs interactive

### Regression
16. All existing E2E tests pass (no iframe-related changes affect main-frame-only pages)

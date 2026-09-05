/**
 * P4 — Method-aware target resolution order (lib/dispatch-order.ts).
 *
 * PURE + DETERMINISTIC. Zero I/O — no browser, extension, CDP, DB, config,
 * timers, or global state. Every decision depends only on the step's own IR
 * fields (target/ref/role/locatorCandidates/value), the compiled IR's binding
 * metadata, and the request's named-data slots (slot NAMES + literal_bound
 * values only — never inline secret VALUES; the values are bound here only as
 * opaque strings the executor injects at dispatch time).
 *
 * Purpose: map a `FlowIRStepV01` onto ONE dispatch decision with an explicit
 * source category, so the executor can route every mutation through the same
 * deterministic ladder:
 *
 *   click / press / hover
 *     semantic identifier ref (`<role>@<index>`) → exact semantic live ref
 *     precise stored locator                     → verified locator
 *     broad cached locator                       → SEMANTIC VALIDATION REQUIRED
 *                                                  (executor confirms a matching
 *                                                  live node before mutating)
 *   fill / type / select
 *     structured locator + named data-slot binding first
 *     semantic live ref only as a validated fallback
 *   navigate
 *     direct route ONLY when the IR target explicitly represents a URL
 *   hover / wait / assert
 *     explicit bounded method contracts (wait/assert → bounded reads)
 *   upload
 *     registered fixture path + locator ladder → `uploadFile` (structured
 *     FileList attach); semantic `file_field@N` ref with no stored locator →
 *     `uploadFile` addressing the Nth `<input type="file">` on the page by
 *     position (the compiled fixture-upload template is a `fill` on
 *     `file_field@N`); UNREGISTERED path / missing slot → reject BEFORE any
 *     browser mutation (no bytes are ever invented)
 *   generic natural-language DOM action
 *     never auto-mapped to a mutation; reject (no supported backend verb)
 *
 * Input-order independence (executor requirement #15): all candidate ordering
 * is canonical — candidates are normalized (dedupe + stable sort by selector
 * text, precise tier before broad tier) so reversing the order of
 * `locatorCandidates` / the `ref` choice provably yields the SAME dispatch
 * decision.
 */
import type {
  DataSlotBindingV01,
  FlowIRStepV01,
} from '../types/browser-task'
import type { IrActionKindV01 } from './ir'
import { isKnownUploadFixture } from './upload-fixtures'

/** Source categories surfaced in step telemetry (never secret). */
export type DispatchCategoryV01 =
  | 'semantic_ref'
  | 'verified_locator'
  | 'flow_memory'
  | 'direct_route'
  | 'bounded_backend_fallback'

/**
 * One resolved dispatch decision. `kind`:
 *   - 'dispatch'     → executor sends a concrete extension action verb with args.
 *   - 'semantic_ref' → executor must snapshot the live AX tree, then act on the
 *                      matched refId (via performActionWithRef).
 *   - 'validated_locator' → executor MUST confirm a live semantic node matches
 *                      the step's role before using the locator; otherwise it
 *                      fails closed (no mutation).
 *   - 'wait' / 'assert' → bounded read contracts, no browser mutation.
 *   - 'reject'       → cannot be executed safely; the executor fails the step.
 */
export interface ResolvedTargetV01 {
  kind: 'dispatch' | 'semantic_ref' | 'validated_locator' | 'wait' | 'assert' | 'reject'
  category: DispatchCategoryV01
  action: IrActionKindV01
  /** Concrete extension verb when `kind` is 'dispatch' / 'validated_locator'. */
  verb?: string
  /** The exact locator to dispatch (precise first, canonical candidate list). */
  locator?: string
  /** Canonical candidate order (deduped + sorted). */
  locatorCandidates?: string[]
  /** Semantic identifier (`<role>@<index>`) when acting on a live ref. */
  semanticRole?: string
  semanticIndex?: number
  /** Direct route for `navigate` (only when the IR target is an explicit URL). */
  url?: string
  /** Bound value for fill/type/select (slot-resolved at execution). */
  value?: string
  /** Wait budget (ms) for 'wait' steps. */
  waitMs?: number
  /** Rejection reason (a canonical, secret-free message). */
  reason?: string
}

/** Result of resolving a step's `value` binding against the request data. */
export type ValueResolutionV01 =
  | { ok: true; value: string }
  | { ok: false; missingSlot: string }

// ---------------------------------------------------------------------------
// Canonical candidate normalization (input-order independent).
// ---------------------------------------------------------------------------

const BROAD_LOCATOR_PATTERNS: RegExp[] = [
  /^\s*\*\s*$/,
  /^\s*(button|a|div|span|li|tr|td|th|table|form|input|select|textarea|img|svg|label|p|h[1-6]|section|article|nav|main|footer|header|aside|ul|ol|dl|dt|dd)\s*$/i,
  /^\s*\[role\s*=\s*["']?(button|link|checkbox|radio|tab|menuitem|option)["']?\s*\]\s*$/i,
  /^\s*\[type\s*=\s*["']?(button|submit|reset)["']?\s*\]\s*$/i,
  /^\s*\.(btn|button|clickable|link|action|item|row|cell|field|input)\s*$/i,
]

/**
 * A locator is BROAD when it is a generic tag/role catch-all that carries no
 * semantic specificity on its own (e.g. `button`, `a`, `[role="button"]`).
 * Broad locators require live semantic target validation before any mutation —
 * a bare `button` is ambiguous across a page and must never be mutated blindly.
 */
export function isBroadLocator(selector: string): boolean {
  const trimmed = (selector ?? '').trim()
  if (!trimmed) return true
  return BROAD_LOCATOR_PATTERNS.some((re) => re.test(trimmed))
}

/** Semantic identifier form emitted by the P3 compiler for non-pattern steps. */
const SEMANTIC_REF_PATTERN = /^([A-Za-z0-9_-]+)@(\d+)$/

/** True when `ref` is a compiler-emitted semantic identifier (`<role>@<index>`). */
export function parseSemanticRef(ref: string): { role: string; index: number } | null {
  const m = SEMANTIC_REF_PATTERN.exec((ref ?? '').trim())
  if (!m) return null
  return { role: m[1], index: Number(m[2]) }
}

/** True when `ref` looks like an explicit URL/route the IR represents. */
export function looksLikeUrl(ref: string): boolean {
  const trimmed = (ref ?? '').trim()
  if (!trimmed) return false
  try {
    const u = new URL(trimmed)
    return u.protocol === 'http:' || u.protocol === 'https:'
  } catch {
    // Relative path / route-key form is still a URL for navigate purposes.
    return trimmed.startsWith('/')
  }
}

/**
 * Canonical normalization of a locator candidate list: dedupe (stable, first
 * occurrence kept) and order by (precise-before-broad, then string). The sort
 * is total and input-order independent — reversing the input array yields the
 * identical output, so a decision made over `locatorCandidates` never depends
 * on how the compiler listed them.
 */
export function normalizeLocatorCandidates(
  candidates: readonly string[] | undefined
): string[] {
  const seen = new Set<string>()
  const ordered: string[] = []
  for (const c of candidates ?? []) {
    const trimmed = typeof c === 'string' ? c.trim() : ''
    if (!trimmed) continue
    if (seen.has(trimmed)) continue
    seen.add(trimmed)
    ordered.push(trimmed)
  }
  return ordered.sort((a, b) => {
    const aBroad = isBroadLocator(a) ? 1 : 0
    const bBroad = isBroadLocator(b) ? 1 : 0
    if (aBroad !== bBroad) return aBroad - bBroad
    return a.localeCompare(b)
  })
}

// ---------------------------------------------------------------------------
// Value binding resolution.
// ---------------------------------------------------------------------------

/**
 * Resolve a step's `value` binding against the request's named-data slots.
 * `slot` bindings require key membership (0 / false / '' still count as
 * present); a missing slot fails closed BEFORE any mutation. `literal_bound`
 * values are compiler-owned non-secret literals and pass through unchanged.
 */
export function resolveDataSlotValue(
  binding: DataSlotBindingV01 | undefined,
  requestData: Record<string, string | number | boolean> | undefined
): ValueResolutionV01 {
  if (!binding) return { ok: true, value: '' }
  if (binding.kind === 'literal_bound') {
    return { ok: true, value: String(binding.value ?? '') }
  }
  const data = requestData ?? {}
  if (!Object.prototype.hasOwnProperty.call(data, binding.slotName)) {
    return { ok: false, missingSlot: binding.slotName }
  }
  return { ok: true, value: String(data[binding.slotName]) }
}

// ---------------------------------------------------------------------------
// Method-aware resolution.
// ---------------------------------------------------------------------------

/** Action kinds that act on a clickable element (semantic-validated). */
const CLICKABLE_KINDS: ReadonlySet<IrActionKindV01> = new Set(['click', 'press', 'hover'])

/** Action kinds that carry a bound value (structured locator + slot binding). */
const VALUED_KINDS: ReadonlySet<IrActionKindV01> = new Set(['fill', 'type', 'select'])

/**
 * Pick the exact dispatch locator from a normalized candidate list: the FIRST
 * precise candidate wins (canonical order guarantees precision-first), which is
 * the "verified stored locator" tier. Broad candidates are NEVER dispatched
 * directly — they downgrade the decision to `validated_locator`.
 */
function pickDispatchLocator(candidates: string[]): { locator: string; broad: boolean } | null {
  if (candidates.length === 0) return null
  const first = candidates[0]
  if (isBroadLocator(first)) return { locator: first, broad: true }
  return { locator: first, broad: false }
}

/**
 * Shared upload decision ladder for (a) the `upload` IR action and (b) valued
 * steps (`fill`/`type`) whose semantic role is `file_field` — the compiled
 * fixture-upload template compiles to a `fill` on `file_field@N` with NO stored
 * locator, so this is the ONLY way the live flow reaches a real upload.
 *
 * Ladder: precise locator → `dispatch` uploadFile; broad locator →
 * `validated_locator` uploadFile; semantic `file_field@N` ref with no locator →
 * `semantic_ref` uploadFile addressing the Nth `<input type="file">` on the page
 * by position; none → reject. `value` MUST already be slot-resolved AND
 * fixture-validated by the caller — this ladder never invents bytes.
 */
function planFileUploadLadder(
  action: IrActionKindV01,
  semantic: { role: string; index: number } | null,
  candidates: string[],
  value: string,
  fallbackRole: string | undefined
): ResolvedTargetV01 {
  const pick = pickDispatchLocator(candidates)
  if (pick && !pick.broad) {
    return {
      kind: 'dispatch',
      category: 'verified_locator',
      action,
      verb: 'uploadFile',
      locator: pick.locator,
      locatorCandidates: candidates,
      value,
    }
  }
  if (pick && pick.broad) {
    return {
      kind: 'validated_locator',
      category: 'bounded_backend_fallback',
      action,
      verb: 'uploadFile',
      locator: pick.locator,
      locatorCandidates: candidates,
      semanticRole: fallbackRole ?? 'file_field',
      value,
    }
  }
  if (semantic && semantic.role === 'file_field') {
    return {
      kind: 'semantic_ref',
      category: 'semantic_ref',
      action,
      verb: 'uploadFile',
      semanticRole: semantic.role,
      semanticIndex: semantic.index,
      value,
    }
  }
  return {
    kind: 'reject',
    category: 'bounded_backend_fallback',
    action,
    reason: `no usable target for upload (no locator, no semantic file_field ref)`,
  }
}

/**
 * Resolve one IR step onto a dispatch decision. The decision is a pure function
 * of `step` + `requestData` slot membership — deterministic and input-order
 * independent. Callers (the executor) then act on the decision: dispatch the
 * verb, snapshot live refs for `semantic_ref`, or fail closed on `reject`.
 */
export function planTargetResolution(
  step: FlowIRStepV01,
  requestData: Record<string, string | number | boolean> | undefined
): ResolvedTargetV01 {
  const action = step.action.type
  const target = step.target

    // ---- navigate: direct route ONLY when the IR target is an explicit URL. ----
    if (action === 'navigate') {
      const ref = target?.ref ?? ''
      if (looksLikeUrl(ref)) {
        return {
          kind: 'dispatch',
          category: 'direct_route',
          action,
          verb: 'navigate',
          url: ref.trim(),
        }
      }
      // B-14 re-review: the rejected ref is echoed nowhere — it can be a
      // hostile string or a URL carrying secrets in its query. The step
      // index/action in the surrounding terminal already identify the step.
      return {
        kind: 'reject',
        category: 'direct_route',
        action,
        reason: 'navigate target is not an explicit URL — refusing to guess a route',
      }
    }

  // ---- wait / assert: explicit bounded read contracts. ----
  if (action === 'wait') {
    return {
      kind: 'wait',
      category: 'bounded_backend_fallback',
      action,
      waitMs: Math.max(0, Math.floor(step.timeoutMs ?? 0)),
    }
  }
  if (action === 'assert') {
    return {
      kind: 'assert',
      category: 'bounded_backend_fallback',
      action,
      semanticRole: target?.role,
    }
  }

  const ref = target?.ref ?? ''
  const semantic = parseSemanticRef(ref)
  const candidates = normalizeLocatorCandidates(target?.locatorCandidates)

  // ---- upload: attach registered fixture bytes via structured uploadFile. ----
  // The extension's structured uploadFile operation receives the fixture descriptor
  // and attaches it to the matched file input (see upload-fixtures).
  // Fail-closed FIRST: a missing slot or an UNREGISTERED fixture path rejects
  // before any browser mutation — no bytes are ever invented. Locator ladder
  // mirrors the valued kinds: precise → dispatch, broad → validated_locator.
  if (action === 'upload') {
    const value = resolveDataSlotValue(step.value, requestData)
    if (!value.ok) {
      return {
        kind: 'reject',
        category: 'bounded_backend_fallback',
        action,
        reason: `missing required named data slot "${value.missingSlot}" — validated before any browser mutation`,
      }
    }
    if (!isKnownUploadFixture(value.value)) {
      return {
        kind: 'reject',
        category: 'bounded_backend_fallback',
        action,
        reason: `upload path "${value.value}" is not a registered fixture — no bytes may be invented`,
      }
    }
    return planFileUploadLadder(action, semantic, candidates, value.value, target?.role)
  }

  // ---- element-scoped click/press/hover ladder. ----
  if (CLICKABLE_KINDS.has(action)) {
    const pick = pickDispatchLocator(candidates)
    if (pick && !pick.broad) {
      // A verified stored selector beats a semantic ref whose index is
      // template-GLOBAL, not per-page: the stored selector was captured on the
      // exact page the step runs against, so it is the highest-confidence tier
      // (bindIrStoredSelectors injects it into the compiled element step).
      return {
        kind: 'dispatch',
        category: 'verified_locator',
        action,
        verb: action,
        locator: pick.locator,
        locatorCandidates: candidates,
      }
    }
    if (semantic && target?.scope === 'element') {
      // No verified locator: compiler-emitted semantic identifier (or a broad
      // cached locator) — resolve an exact live ref as the validated tier.
      return {
        kind: 'semantic_ref',
        category: 'semantic_ref',
        action,
        semanticRole: semantic.role,
        semanticIndex: semantic.index,
      }
    }
    if (pick && pick.broad) {
      return {
        kind: 'validated_locator',
        category: 'bounded_backend_fallback',
        action,
        verb: action,
        locator: pick.locator,
        locatorCandidates: candidates,
        semanticRole: target?.role ?? 'element',
      }
    }
    // A click with neither a semantic identifier nor any locator is unusable.
    return {
      kind: 'reject',
      category: 'bounded_backend_fallback',
      action,
      reason: `no usable target for ${action} (no live ref, no locator)`,
    }
  }

  // ---- fill/type/select: structured locator + named data-slot binding FIRST. ----
  if (VALUED_KINDS.has(action)) {
    const value = resolveDataSlotValue(step.value, requestData)
    if (!value.ok) {
      return {
        kind: 'reject',
        category: 'bounded_backend_fallback',
        action,
        reason: `missing required named data slot "${value.missingSlot}" — validated before any browser mutation`,
      }
    }
    // A valued step bound to a semantic file_field (the compiled fixture-upload
    // template is `fill` on `file_field@N`, no stored locator) is an upload:
    // fixture-gate the value, then run the upload ladder. Without this branch
    // the ref would be sent to performActionWithRef('fill') — a silent no-op on
    // a file input.
    const roleIsFileField = semantic?.role === 'file_field' || target?.role === 'file_field'
    if (roleIsFileField) {
      if (!isKnownUploadFixture(value.value)) {
        return {
          kind: 'reject',
          category: 'bounded_backend_fallback',
          action,
          reason: `upload path "${value.value}" is not a registered fixture — no bytes may be invented`,
        }
      }
      return planFileUploadLadder(action, semantic, candidates, value.value, target?.role)
    }
    const pick = pickDispatchLocator(candidates)
    if (pick && !pick.broad) {
      return {
        kind: 'dispatch',
        category: 'verified_locator',
        action,
        verb: action,
        locator: pick.locator,
        locatorCandidates: candidates,
        value: value.value,
      }
    }
    if (semantic && target?.scope === 'element') {
      // No verified locator: semantic live ref as a VALIDATED fallback.
      return {
        kind: 'semantic_ref',
        category: 'semantic_ref',
        action,
        semanticRole: semantic.role,
        semanticIndex: semantic.index,
        value: value.value,
      }
    }
    if (pick && pick.broad) {
      return {
        kind: 'validated_locator',
        category: 'bounded_backend_fallback',
        action,
        verb: action,
        locator: pick.locator,
        locatorCandidates: candidates,
        semanticRole: target?.role ?? 'element',
        value: value.value,
      }
    }
    return {
      kind: 'reject',
      category: 'bounded_backend_fallback',
      action,
      reason: `no usable target for ${action} (no verified locator, no live ref)`,
    }
  }

  // ---- unknown action kinds (defense in depth — validateFlowIr rejects first). ----
  return {
    kind: 'reject',
    category: 'bounded_backend_fallback',
    action,
    reason: `action "${action}" has no supported dispatch mapping`,
  }
}

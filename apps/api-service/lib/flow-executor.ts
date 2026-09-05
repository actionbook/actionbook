/**
 * P4 — Deterministic flow executor (lib/flow-executor.ts).
 *
 * Consumes a validated `BrowserFlowIRV01` and drives ONE bound extension
 * runtime/session/tab/lease through its steps in IR order, mutating only after
 * the live screen has been synchronized AND classified, and failing closed the
 * instant any step, binding check, or screen check fails.
 *
 * ZERO I/O of its own. Every browser interaction (acquire, binding
 * verification, dispatch, live-ref snapshot, semantic action), every screen
 * synchronization, and every screen classification arrives through the
 * injected `ExecutorRuntimeV01`. This module never imports the database, global
 * config, the MCP dispatcher, or any browser-launch facility — so it provably
 * cannot launch a fallback browser and it is fully deterministic against a fake
 * runtime.
 *
 * Frozen-result discipline (P0 §3 / §7): every path returns a terminal
 * `BrowserTaskResultV01` with a canonical failure reason — never a thrown
 * exception, never a partial stream. Transport completion alone NEVER becomes
 * `status:'success'`: a task only succeeds when all steps executed AND the final
 * live screen resolves to the IR's `expectedFinalScreen` (business destination)
 * AND every IR step postcondition AND every request assertion evaluates to
 * `passed` (P5, lib/postconditions.ts). Unsupported assertion shapes fail
 * closed to the frozen canonical `contract_conflict` reason.
 */
import { planTargetResolution, resolveDataSlotValue } from './dispatch-order'
import { redactText } from './redaction'
import { statusForReason } from './browser-task'
import type { BrowserTaskVerification } from './browser-task'
import { getUploadFixture } from './upload-fixtures'
import {
  defaultP5Evaluator,
  interpretFieldProbe,
  interpretRowKeyProbe,
  interpretRowKeyProbeWithScopes,
  operatorFromTaskAssertion,
  resolveStepPostcondition,
  summarizeAssertions,
  type AssertionCheckV01,
  type AssertionVerdictV01,
  type FieldProbeOutcomeV01,
  type P5AssertionEvaluatorV01,
  type PostconditionEvidenceV01,
} from './postconditions'
import type { ResolvedTargetV01 } from './dispatch-order'
import type { ResolverSnapshotV01, ScreenResolutionV01, StoredScreenV01 } from './screen-resolver'
import type { IrActionKindV01 } from './ir'
import { scrubSemanticRoleForLog } from './ir'
import type {
  BrowserFlowIRV01,
  BrowserTaskFailureReasonV01,
  BrowserTaskRequestV01,
  BrowserTaskResultV01,
  ExecutionPathV01,
  FlowIRStepV01,
  RecoveryReasonV01,
  ScreenIdentityV01,
} from '../types/browser-task'

// ---------------------------------------------------------------------------
// Bounds (deterministic — never derived from wall clock or global state).
// ---------------------------------------------------------------------------

export const FLOW_EXECUTOR_LIMITS = {
  /** Mirror of COMPILER_LIMITS.stepsMaxTotal (P3) — an IR beyond this is malformed. */
  stepsMaxTotal: 64,
  /** Max UTF-8 bytes for one bound value (fill/type/select/press). */
  valueBytesMax: 8192,
  /** Max live AX refs a semantic snapshot may consume. */
  snapshotRefsMax: 200,
  /** Default task budget when neither the request nor the runner supplies one. */
  defaultTaskTimeoutMs: 120_000,
  /** Standard stabilization poll budget after a mutation. */
  standardStabilizeMs: 3_000,
  /** Longer stabilization budget for login/save/submit transitions. */
  slowTransitionStabilizeMs: 15_000,
  /** Fixed, deterministic stabilization poll interval. */
  stabilizePollMs: 250,
  /** Fixed, deterministic assertion poll interval (P5). */
  assertionPollMs: 250,
  /** Fixed, deterministic backoff between bounded read retries. */
  readRetryBackoffMs: 100,
  /** Hard ceiling on read-retry attempts regardless of recovery policy. */
  maxReadRetryAttempts: 2,
} as const

// ---------------------------------------------------------------------------
// Runtime contract (injected — never imported by this module).
// ---------------------------------------------------------------------------

/** A live accessibility-tree node as surfaced by the bound runtime. */
export interface LiveRefNodeV01 {
  refId: string
  role: string
  name?: string
}

/** Non-secret identity of the one bound runtime session/tab/lease. */
export interface ExecutorBindingV01 {
  tabId?: string | number
  sessionId?: string
  leaseId?: string
  profileKey?: string
  /** Non-secret stable identity for telemetry (may be a hash of session/tab). */
  identityKey?: string
}

export type RuntimeAcquireReasonV01 =
  | 'no_extension'
  | 'extension_timeout'
  | 'scope_denied'
  | 'error'

export interface RuntimeAcquireResultV01 {
  ok: boolean
  reason?: RuntimeAcquireReasonV01
  binding?: ExecutorBindingV01
}

/** Structural dispatch result (mirrors the extension bridge envelope). */
export interface FlowDispatchErrorV01 {
  code: string
  message: string
  hint?: string
}

export type FlowDispatchResultV01 =
  | { ok: true; result: unknown }
  | { ok: false; error: FlowDispatchErrorV01 }

/**
 * Origin+path of an absolute http(s) url — scheme + host + path with any
 * trailing slash normalized, query/fragment dropped. Two screens sharing this
 * are "the same place" for the replay twin tolerance: the draft-local pool mints
 * separate identities for pre/post content variants of one page (a client-side
 * filtered list vs the same list unfiltered differ only in ROW COUNT, hence in
 * signature) and those tie inside the resolver's ambiguity band, which would
 * otherwise fail closed every classification gate. Module-scope so BOTH the
 * pre-mutation gate and the destination gate can use it. Returns null for a
 * non-absolute or unparseable url so callers fail closed on unknown geometry.
 */
function originPath(raw: string | null | undefined): string | null {
  if (!raw || !/^https?:\/\//i.test(raw)) return null
  try {
    const u = new URL(raw)
    return `${u.protocol}//${u.host}${u.pathname.replace(/\/+$/, '') || '/'}`
  } catch {
    return null
  }
}

/**
 * Route template of an absolute url — origin+path with a trailing NUMERIC id
 * segment generalized to `*`, so two records of one parameterized route
 * (`/admin/clients/client/13845` vs `/…/13846`) are the "same place." A flow
 * that CREATES a record can never land on the learned destination's exact path,
 * because every run mints a new id; without this the destination gate rejects a
 * structurally-correct replay forever (measured: `expPath=…/13845` vs
 * `livePath=…/13846`, both t3=1.000, ambiguity_band). Deliberately conservative:
 * a single-segment path is left intact (so `/42` and `/77` stay distinct — a bare
 * numeric root is an identifier, not a record id under a route), and only a
 * numeric segment that FOLLOWS other path segments is generalized.
 */
function routeTemplate(raw: string | null | undefined): string | null {
  if (!raw || !/^https?:\/\//i.test(raw)) return null
  try {
    const u = new URL(raw)
    const path = u.pathname.replace(/\/+$/, '') || '/'
    const segs = path.split('/')
    const meaningful = segs.filter((s) => s.length > 0).length
    // A single-segment path is an identifier, not a record id under a route —
    // leave it intact so `/42` and `/77` stay distinct (guards the fail-closed
    // negatives). Only generalize a numeric segment that FOLLOWS other segments.
    if (meaningful < 2) return `${u.protocol}//${u.host}${path}`
    const norm = segs.map((s, idx) => (idx > 0 && /^\d+$/.test(s) ? '*' : s))
    return `${u.protocol}//${u.host}${norm.join('/')}`
  } catch {
    return null
  }
}

/**
 * Is a `screen_ambiguous` resolver verdict benign for a replay, because the LIVE
 * screen's ROUTE (parameter-insensitive) is provably one of the episode's OWN
 * endpoint routes? A genuine drift to any other page keeps its own route and
 * still fails closed.
 */
function benignTwinAmbiguity(
  resolution: { status: string; reason?: string },
  liveUrl: string | null | undefined,
  allowedRoutes: ReadonlySet<string>
): boolean {
  if (resolution.status === 'resolved') return false
  if ((resolution.reason ?? '') !== 'screen_ambiguous') return false
  const liveRoute = routeTemplate(liveUrl)
  return liveRoute !== null && allowedRoutes.has(liveRoute)
}

/**
 * The injected runtime seam. The production runner wires this to exactly one
 * extension connection/session/tab/lease via `dispatchToExtension`; tests wire
 * it to a fake. `classifyScreen` is the pure P2 resolver over the projections
 * the runner already loaded — it never performs I/O itself.
 */
export interface ExecutorRuntimeV01 {
  /** Bind ONE extension session/tab/lease for the entire task. */
  acquire(): Promise<RuntimeAcquireResultV01>
  /** True when the bound connection/session/tab/lease is still valid. */
  verifyBinding(): Promise<boolean>
  /** Send one extension action verb. Never throws. */
  dispatch(
    actionType: string,
    args: Record<string, unknown>,
    opts?: { timeoutMs?: number }
  ): Promise<FlowDispatchResultV01>
  /** Capture a live, bounded snapshot of the current screen. Never throws. */
  syncScreen(): Promise<ResolverSnapshotV01>
  /** Classify a snapshot against the loaded projections (pure, synchronous). */
  classifyScreen(snapshot: ResolverSnapshotV01): ScreenResolutionV01
  /** Snapshot the live accessibility tree (bounded). Never throws. */
  snapshotRefs(): Promise<LiveRefNodeV01[]>
  /** Act on a matched live ref (`performActionWithRef`). Never throws. */
  performActionWithRef(
    refId: string,
    method: string,
    args: unknown[]
  ): Promise<FlowDispatchResultV01>
  /**
   * Rebind the stored-screen candidate pool `classifyScreen` scores against.
   * Optional: only runtimes that own a mutable pool expose it. Guided draft
   * replay binds its draft-local two-screen pool around `executeFlow` and
   * restores the caller's projection afterwards, so replay classification is
   * scoped to the draft instead of the (possibly ambiguous) global pool.
   */
  bindCandidates?(candidates: readonly StoredScreenV01[]): void
  /** Injectable clock (deterministic in tests). */
  now(): number
}

// ---------------------------------------------------------------------------
// Semantic target matching (pure, deterministic).
// ---------------------------------------------------------------------------

const CLICKABLE_ROLES: ReadonlySet<string> = new Set([
  'button',
  'link',
  'checkbox',
  'radio',
  'menuitem',
  'tab',
  'option',
  'switch',
  'combobox',
  'listbox',
  'textbox',
  'textarea',
  'searchbox',
])

/**
 * The roles a valued write (fill/type/select) may land on. P5 rework round 6:
 * `checkbox` was removed — its business state is `checked`, not `value`, and a
 * valued write cannot represent it. The round-6 empirical showed fill("true")
 * changing only the box's .value (still unchecked) while the transport
 * reported success: a transport-success false-positive. Checked-state
 * controls (checkbox/radio/switch) are toggled via `click` — the compiler
 * targets them through `toggle_field` → CLICKABLE_ROLES, never via fill.
 */
const FILLABLE_ROLES: ReadonlySet<string> = new Set([
  'textbox',
  'combobox',
  'textarea',
  'searchbox',
  'spinbutton',
])

/**
 * P5 rework round 6: checked-state controls whose business value is `checked`,
 * not a typed value. They must never satisfy a valued write's semantic match —
 * neither as a base FILLABLE_ROLES member nor as an explicit target role.
 *
 * P5 rework round 7: this role-level exclusion is defense-in-depth, not the
 * whole contract. The page op (bannerless-page.js) is the authority that must
 * ALSO refuse fill/type on checkbox/radio/switch BY ROLE — regardless of
 * tag/type/contenteditable — because a custom ARIA checked control (BUTTON/DIV
 * role=switch, INPUT type=text role=switch) is not caught by an input[type]
 * filter alone. The round-7 empirical rode fill("false") on an
 * aria-checked=true INPUT[type=text][role=switch] as a confirmed no-op.
 */
const CHECKED_TYPE_ROLES: ReadonlySet<string> = new Set([
  'checkbox',
  'radio',
  'switch',
])

const VALUED_ACTION_SET: ReadonlySet<IrActionKindV01> = new Set(['fill', 'type', 'select', 'upload'])

/**
 * The P3 compiler emits SYNTHETIC form roles (branch_action, auth_entry,
 * toggle_field, text_field, secret_field, search_query, file_field) for
 * non-pattern steps — these are affordance descriptors, not AX roles. Map each
 * synthetic role onto the acceptable real AX-role set for semantic matching.
 */
const SYNTHETIC_ROLE_ROLES: Record<string, ReadonlySet<string>> = {
  text_field: FILLABLE_ROLES,
  secret_field: FILLABLE_ROLES,
  search_query: FILLABLE_ROLES,
  file_field: FILLABLE_ROLES,
  branch_action: CLICKABLE_ROLES,
  auth_entry: CLICKABLE_ROLES,
  toggle_field: new Set(['checkbox', 'switch']),
}

/**
 * The set of real AX roles that satisfy a semantic target for an action kind.
 * A synthetic form role maps onto its affordance set; a bare real AX role
 * (e.g. `button`) matches exactly that role; `element` / absent means "any
 * interactive node" via the action's own base set.
 *
 * P5 rework round 6: checked-state controls (checkbox/radio/switch) can never
 * satisfy a VALUED write's match — not from the base set (checkbox is out of
 * FILLABLE_ROLES) and not as an explicit target role either. Their business
 * state is `checked`; a fill/type/select value cannot represent it and the
 * step must fail closed pre-dispatch ("no live semantic target") instead of
 * mutating a control that silently ignores the write.
 */
export function acceptableRoles(
  role: string | undefined,
  action: IrActionKindV01
): ReadonlySet<string> {
  const valued = VALUED_ACTION_SET.has(action)
  const base = valued ? FILLABLE_ROLES : CLICKABLE_ROLES
  const r = (role ?? '').trim()
  const set = !r || r === 'element' ? base : SYNTHETIC_ROLE_ROLES[r] ?? new Set([r])
  if (!valued) return set
  const filtered = new Set<string>()
  for (const candidate of set) {
    if (!CHECKED_TYPE_ROLES.has(candidate)) filtered.add(candidate)
  }
  return filtered
}

/**
 * Resolve `<role>@<index>` onto a concrete live node: the `index`-th (0-based)
 * live node whose AX role is acceptable for the action, in snapshot order.
 * Returns null when no such node exists (the executor fails closed).
 */
export function findLiveRef(
  nodes: readonly LiveRefNodeV01[],
  role: string | undefined,
  index: number | undefined,
  action: IrActionKindV01
): LiveRefNodeV01 | null {
  const roles = acceptableRoles(role, action)
  const matches: LiveRefNodeV01[] = []
  for (const n of nodes) {
    if (roles.has(n.role ?? '')) matches.push(n)
  }
  const at = Math.max(0, index ?? 0)
  return matches[at] ?? null
}

/**
 * B-8 (warm-benchmark, local fixture 2026-09-03): recorded target names are
 * slot-TOKENIZED ('Open $company — $email'), but `findLiveRef` binds the step
 * by absolute role position. As soon as the list's order or size differs
 * between the learned session and the replay, the same index names a
 * DIFFERENT business object — measured live: `button@26` recorded on a 27-row
 * list opened the previous run's customer on every later run (the terminal
 * assertion caught it, then every warm run paid a guided session; the engine
 * never converged back to zero-model). When the recorded template renders
 * with this run's data and EXACTLY ONE acceptable live node carries that
 * accessible name, prefer it — the positional ref stays the fallback so a
 * page without the (unique) named element behaves exactly as before.
 * Ambiguity (2+ live matches) keeps the positional binding: a name shared by
 * several rows proves nothing about which one the episode meant.
 */
export function nameAnchoredLiveRef(
  nodes: readonly LiveRefNodeV01[],
  role: string | undefined,
  action: IrActionKindV01,
  nameTemplate: string | undefined,
  data: Record<string, unknown> | undefined
): LiveRefNodeV01 | null {
  const rendered = renderSlotTemplate(nameTemplate, data)
  if (!rendered) return null
  const roles = acceptableRoles(role, action)
  const want = normalizeNameForAnchor(rendered)
  if (!want) return null
  const hits = nodes.filter(
    (n) => roles.has(n.role ?? '') && n.name && normalizeNameForAnchor(n.name) === want
  )
  return hits.length === 1 ? hits[0] : null
}

/** Whitespace-normalize for accessible-name comparison. */
function normalizeNameForAnchor(name: string): string {
  return name.replace(/\s+/g, ' ').trim()
}

/**
 * Substitute `$slot` tokens in a recorded name with this run's data values.
 * Returns null when the template holds no token (a literal name proves
 * nothing about drift) or when any token cannot be resolved — an unresolved
 * placeholder would match nothing and must not shadow the positional binding.
 */
function renderSlotTemplate(
  template: string | undefined,
  data: Record<string, unknown> | undefined
): string | null {
  if (!template || !/\$[a-zA-Z]/.test(template)) return null
  let unresolved = false
  const rendered = template.replace(/\$([a-zA-Z][a-zA-Z0-9_]*)/g, (_m, key: string) => {
    const value = data?.[key]
    if (typeof value === 'string' && value.trim()) return value
    unresolved = true
    return _m
  })
  return unresolved ? null : rendered
}

// ---------------------------------------------------------------------------
// Deterministic policy helpers.
// ---------------------------------------------------------------------------

const TRANSIENT_CODES = new Set(['NO_EXTENSION', 'EXTENSION_TIMEOUT', 'EXTENSION_ERROR'])

/**
 * Closed allowlist of extension/bridge/page error codes that may appear in
 * structured diagnostics (B-14 re-review). Extension `error.message` strings
 * routinely embed the raw selector (`element has zero size ...: ${selector}`),
 * refIds, and even page exception text — page content that must never reach
 * terminal notes or logs (`note(secrets, ...)` only replaces known
 * request-data values, so selector/page canaries survive it). Diagnostics
 * derive from the code only; anything outside this set degrades to a
 * constant. Members mirror the fixed vocabularies in `extension-bridge`,
 * `browser-run-task`/`browser-lease`, and the extension page scripts.
 */
const LOG_SAFE_DISPATCH_ERROR_CODES: ReadonlySet<string> = new Set([
  // Bridge transport/session codes.
  'NO_EXTENSION',
  'BACKEND_MISMATCH',
  'SCOPE_DENIED',
  'EXTENSION_TIMEOUT',
  'EXTENSION_ERROR',
  'DAEMON_UNREACHABLE',
  'DAEMON_REJECTED',
  // Durable lease codes.
  'USER_MISMATCH',
  'LEASE_SESSION_REQUIRED',
  'ABORTED',
  'BUDGET_EXHAUSTED',
  'LEASE_BINDING_INVALID',
  'LEASE_REQUIRED',
  'STALE_LEASE',
  'RELEASE_IN_PROGRESS',
  'RELEASE_ACK_PENDING',
  'RELEASE_FAILED',
  'INVALID_LEASE_TTL',
  'DB_UNAVAILABLE',
  'LEASE_CLEANUP_PENDING',
  'LEASE_ACQUIRE_IN_PROGRESS',
  'LEASE_BACKEND_LOST',
  'LEASE_RELEASE_IN_PROGRESS',
  'LEASE_QUERY_FAILED',
  // Extension CDP/page codes (background.js / bannerless-page.js).
  'CDP_ERROR',
  'TAB_CLOSED',
  'NO_ACTIVE_TAB',
  'CDP_TAB_HIDDEN',
  'CDP_EVAL_ERROR',
  'CDP_NOT_INTERACTABLE',
  'REF_NOT_FOUND',
  'NOT_INTERACTABLE',
  'UNSUPPORTED',
  'INVALID_UPLOAD',
  'NO_FILE_INPUT',
  'INVALID_SELECTOR',
  'SCRIPT_ERROR',
  'UPLOAD_SET_FAILED',
])

/** Allowlisted code or the constant placeholder — never the raw value. */
function dispatchErrorCodeForLog(code: string | undefined): string {
  return typeof code === 'string' && LOG_SAFE_DISPATCH_ERROR_CODES.has(code) ? code : 'UNKNOWN'
}

/**
 * Centralized failure diagnostic for both extension carriers (`dispatch` and
 * `performActionWithRef`): fixed tokens from the allowlisted code plus the
 * closed action vocabulary and step index. The raw `error.message` is never
 * used — it carries selectors, refIds, and page exception text. (The `verb`
 * is safe: every dispatch runs after `validateFlowIr` accepted the step's
 * action against the closed `IR_ACTION_KINDS` vocabulary.)
 */
function extensionFailureDetail(
  stepIndex: number,
  verb: string,
  channel: 'dispatch' | 'perform',
  code: string | undefined
): string {
  return (
    `extension ${verb} ${channel} failed for step ${stepIndex} ` +
    `(${dispatchErrorCodeForLog(code)}) — failed closed`
  )
}

/** True when a dispatch error is a transient transport failure (retryable). */
function isTransientCode(code: string | undefined): boolean {
  return !!code && TRANSIENT_CODES.has(code)
}

/** Map an extension bridge error code onto the frozen §7 failure reason. */
export function mapDispatchErrorCode(
  code: string | undefined
): BrowserTaskFailureReasonV01 {
  switch (code) {
    case 'NO_EXTENSION':
      return 'no_extension'
    case 'SCOPE_DENIED':
      return 'scope_denied'
    case 'EXTENSION_TIMEOUT':
      return 'extension_timeout'
    case 'DAEMON_UNREACHABLE':
      return 'daemon_unreachable'
    case 'REF_NOT_FOUND':
      return 'step_failed'
    default:
      return 'error'
  }
}

/** Deterministic read-retry budget from the IR's recovery policy. */
export function readRetryBudget(
  recoveryPolicy: BrowserFlowIRV01['recoveryPolicy']
): number {
  if (recoveryPolicy === 'single_retry_read') return 1
  if (recoveryPolicy === 'bounded') return FLOW_EXECUTOR_LIMITS.maxReadRetryAttempts
  return 0
}

const SLOW_TRANSITION_RE =
  /login|auth|sign\s*[-_]?\s*in|signin|save|submit|create|register|checkout|confirm|upload|purchase/i

/** Longer bounded stabilization budget applies to login/save/submit transitions. */
export function isSlowTransition(step: FlowIRStepV01): boolean {
  if (step.action.type === 'navigate') return true
  const role = step.target?.role ?? ''
  const locators = step.target?.locatorCandidates ?? []
  if (SLOW_TRANSITION_RE.test(role)) return true
  return locators.some((l) => SLOW_TRANSITION_RE.test(l))
}

/** One `metrics.path` token per executed step, from the dispatch decision. */
function pathTokenFor(plan: ResolvedTargetV01): ExecutionPathV01 {
  if (plan.kind === 'semantic_ref') return 'live_ref'
  if (plan.kind === 'validated_locator') return 'locator'
  if (plan.kind === 'dispatch') return plan.category === 'direct_route' ? 'route_family' : 'locator'
  return 'screen_cache' // wait / assert — bounded reads of the current screen
}

// ---------------------------------------------------------------------------
// IR validation (full, before ANY mutation).
// ---------------------------------------------------------------------------

const IR_ACTION_KINDS: readonly IrActionKindV01[] = [
  'navigate',
  'click',
  'fill',
  'type',
  'press',
  'select',
  'hover',
  'upload',
  'wait',
  'assert',
]

const VALUE_CAPABLE_KINDS: ReadonlySet<IrActionKindV01> = new Set([
  'fill',
  'type',
  'select',
  'press',
  'upload',
])

/**
 * The subset of value-capable kinds that WRITE a value into a form control and
 * therefore are verified by the unconditional write-read-back gate. `press`
 * sends a key (no value to read back) and `upload` carries its own
 * authoritative attach outcome (`writeVerified`), so neither is gated here.
 *
 * P5 rework round 4, finding 2: these kinds are ALWAYS mutations. The step's
 * `mutation` flag is the idempotency designation for the retry policy (see
 * `runStepWithRetry`), and it must never classify a value-writing step as
 * non-mutating: the round-4 probe showed a `fill` with `mutation:false` sailed
 * past the write gate (no read-back ran) because the gate keyed on the flag.
 * The validator now rejects the mismatch, and the gate keys on action kind as
 * defense in depth. `upload` is intentionally excluded: it carries its own
 * authoritative attach outcome (`writeVerified`), so it never reaches the
 * read-back gate, and it is likewise not subject to the validator rule above.
 */
const VALUED_WRITE_KINDS: ReadonlySet<IrActionKindV01> = new Set([
  'fill',
  'type',
  'select',
])

/**
 * P5 rework round 4, finding 1 — the identity-bound read-back verb. The
 * extension resolves the SAME backendDOMNodeId the action was dispatched
 * through (DOM.resolveNode + Runtime.callFunctionOn) and compares the value
 * ON THAT NODE — boolean-only result, never echoes the value. The verb is
 * implemented in `packages/chrome-extension/background.js` (bannerless-v1
 * router case); a DOM-order re-selection (querySelectorAll + role/index) can
 * NEVER be the read-back: the reviewer's probe showed a hidden aria-hidden
 * clone field with a pre-existing value passing such a probe while the real
 * node stayed empty.
 */
const SEMANTIC_REF_PROBE_VERB = 'readRefValue' as const

/**
 * Canonical `readRefValue` reply envelope — the SINGLE normalization boundary
 * for every consumer (`verifySemanticWrite` here and the guided-discovery
 * valued-write confirmation both pass their full dispatch envelope in).
 *
 * What the live extension router actually answers (locked by
 * `packages/chrome-extension/test/read-ref-value.test.js`, router-case
 * `toEqual`, and cross-checked core-side by the contract test in
 * `test/flow-executor.test.ts` that parses background.js itself):
 *   { ok: true, result: { refId: string, verified: boolean, match: string } }
 * The page op's boolean-only verdict `{ ok, code }` is flattened into
 * `verified` by the router; the transport frame's `ok` says nothing about the
 * value, and no raw field value ever crosses this boundary.
 *
 * The legacy pre-flatten page envelope is still accepted (older routers and
 * the direct page result shape):
 *   { ok: true, result: { value: { ok: boolean, code?: string } } }
 *
 * Fail-closed interpretation (`interpretSemanticRefValueProbe`):
 *  - transport `ok !== true`                      -> 'unverifiable'
 *  - `verified === true` / `verified === false`   -> 'ok' / 'mismatch'
 *  - legacy `result.value.ok` true/false          -> 'ok' / 'mismatch'
 *  - both verdicts present and DISAGREEING        -> 'unverifiable' (a
 *    conflicting dual envelope is malformed, never resolved by preference)
 *  - any non-boolean verdict field, missing
 *    verdict, malformed envelope                   -> 'unverifiable'
 * A page-level `ok:false` (VALUE_MISMATCH, NO_TARGET, …) reaches core only as
 * `verified:false` from the live router and maps to 'mismatch' — a definite
 * "the node does not hold the value"; identity loss is still fail-closed
 * either way, and a transport-level rejection stays 'unverifiable' (the
 * callers gate it before interpretation).
 */
export function interpretSemanticRefValueProbe(raw: unknown): FieldProbeOutcomeV01 {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return 'unverifiable'
  const envelope = raw as { ok?: unknown; result?: unknown }
  if (envelope.ok !== true) return 'unverifiable'
  const result = envelope.result
  if (!result || typeof result !== 'object' || Array.isArray(result)) return 'unverifiable'
  const page = result as Record<string, unknown>

  // Canonical router verdict — it must be a real boolean to carry any verdict.
  const hasVerified = Object.prototype.hasOwnProperty.call(page, 'verified')
  const verified = page.verified
  if (hasVerified && typeof verified !== 'boolean') return 'unverifiable'

  // Legacy page verdict — a non-object `value` (a raw field value wearing the
  // verdict slot) decides nothing and leaks nothing.
  const value = page.value
  const legacyOk =
    value && typeof value === 'object' && !Array.isArray(value)
      ? (value as { ok?: unknown }).ok
      : undefined
  const hasLegacy = legacyOk === true || legacyOk === false

  if (hasVerified && hasLegacy && verified !== legacyOk) return 'unverifiable'
  if (hasVerified) return verified === true ? 'ok' : 'mismatch'
  if (hasLegacy) return legacyOk === true ? 'ok' : 'mismatch'
  return 'unverifiable'
}

export type FlowIrValidationOutcomeV01 =
  | { ok: true }
  | { ok: false; reason: BrowserTaskFailureReasonV01; detail: string }

/** UTF-8 byte length (portable across Buffer / TextEncoder). */
function utf8Bytes(s: string): number {
  try {
    return Buffer.byteLength(s, 'utf8')
  } catch {
    return new TextEncoder().encode(s).length
  }
}

/**
 * Reject malformed / unsafe IR BEFORE the executor touches the browser:
 * unsupported version, non-contiguous or non-unique step indices, too many
 * steps, unknown action kinds, invalid timeouts, invalid target shapes, value
 * bindings on non-value-capable actions, press without a bound key, missing
 * required named data slots, oversized values, and out-of-range
 * `destination_screen_score` postcondition thresholds. Every check is pure.
 */
export function validateFlowIr(
  ir: unknown,
  requestData: Record<string, string | number | boolean> | undefined
): FlowIrValidationOutcomeV01 {
  if (!ir || typeof ir !== 'object') {
    return { ok: false, reason: 'contract_conflict', detail: 'IR is not an object' }
  }
  const candidate = ir as Partial<BrowserFlowIRV01>
  if (candidate.version !== '0.1') {
    return {
      ok: false,
      reason: 'contract_conflict',
      detail: `unsupported IR version ${String(candidate.version)} — only 0.1 is accepted`,
    }
  }
  if (!Array.isArray(candidate.steps)) {
    return { ok: false, reason: 'contract_conflict', detail: 'IR steps is not an array' }
  }
  if (candidate.steps.length > FLOW_EXECUTOR_LIMITS.stepsMaxTotal) {
    return {
      ok: false,
      reason: 'contract_conflict',
      detail: `IR has ${candidate.steps.length} steps — exceeds the ${FLOW_EXECUTOR_LIMITS.stepsMaxTotal} bound`,
    }
  }
  if (
    candidate.idempotency !== undefined &&
    candidate.idempotency !== 'read' &&
    candidate.idempotency !== 'idempotent_mutation' &&
    candidate.idempotency !== 'stateful_mutation'
  ) {
    return {
      ok: false,
      reason: 'contract_conflict',
      detail: `invalid idempotency ${String(candidate.idempotency)}`,
    }
  }
  if (
    candidate.recoveryPolicy !== undefined &&
    candidate.recoveryPolicy !== 'none' &&
    candidate.recoveryPolicy !== 'single_retry_read' &&
    candidate.recoveryPolicy !== 'bounded'
  ) {
    return {
      ok: false,
      reason: 'contract_conflict',
      detail: `invalid recoveryPolicy ${String(candidate.recoveryPolicy)}`,
    }
  }

  for (let i = 0; i < candidate.steps.length; i++) {
    const step = candidate.steps[i]
    if (!step || typeof step !== 'object') {
      return { ok: false, reason: 'contract_conflict', detail: `step ${i} is not an object` }
    }
    // Indices must be exactly 0..n-1 in order (the compiler emits contiguous,
    // unique, ascending indices — any deviation is a malformed IR).
    if (step.index !== i) {
      return {
        ok: false,
        reason: 'contract_conflict',
        detail: `step ${i} has non-contiguous index ${String(step.index)}`,
      }
    }
    const actionType = step.action?.type
    if (!IR_ACTION_KINDS.includes(actionType)) {
      return {
        ok: false,
        reason: 'contract_conflict',
        detail: `step ${i} has unknown action type ${String(actionType)}`,
      }
    }
    // P5 rework round 4, finding 2: the mutation flag must agree with the
    // action kind for every value-writing step. A `fill`/`type`/`select`
    // marked `mutation:false` bypassed the write-verification gate (and the
    // pre-mutation binding check) — the reviewer's probe executed such an IR
    // and got `success` with no read-back. Reject the mismatch; the executor
    // gate additionally keys on action kind, never on this flag alone.
    // The flag itself is a required contract field — a missing/non-boolean
    // flag is a malformed IR (it would silently make a mutation retryable).
    if (typeof step.mutation !== 'boolean') {
      return {
        ok: false,
        reason: 'contract_conflict',
        detail: `step ${i} has a missing or non-boolean mutation flag`,
      }
    }
    if (VALUED_WRITE_KINDS.has(actionType) && step.mutation !== true) {
      return {
        ok: false,
        reason: 'contract_conflict',
        detail: `step ${i} (${actionType}) writes a value into a form control and must be marked mutation:true — got ${String(step.mutation)}`,
      }
    }
    if (
      typeof step.timeoutMs !== 'number' ||
      !Number.isFinite(step.timeoutMs) ||
      step.timeoutMs < 0
    ) {
      return {
        ok: false,
        reason: 'contract_conflict',
        detail: `step ${i} has invalid timeoutMs ${String(step.timeoutMs)}`,
      }
    }
    if (step.target !== undefined) {
      if (
        !step.target ||
        (step.target.scope !== 'screen' && step.target.scope !== 'element')
      ) {
        return {
          ok: false,
          reason: 'contract_conflict',
          detail: `step ${i} has invalid target scope ${String(step.target?.scope)}`,
        }
      }
    }
    if (actionType === 'navigate') {
      if (!step.target || step.target.scope !== 'screen') {
        return {
          ok: false,
          reason: 'contract_conflict',
          detail: `navigate step ${i} requires a screen-scoped target`,
        }
      }
    }
    // Value bindings: only value-capable actions may carry them; the binding
    // itself must be well-formed.
    if (step.value !== undefined) {
      if (!VALUE_CAPABLE_KINDS.has(actionType)) {
        return {
          ok: false,
          reason: 'contract_conflict',
          detail: `step ${i} (${actionType}) carries a value binding but only ${[...VALUE_CAPABLE_KINDS].join('/')} may`,
        }
      }
      if (
        (step.value.kind === 'slot' && !step.value.slotName) ||
        (step.value.kind !== 'slot' &&
          step.value.kind !== 'literal_bound') ||
        (step.value.kind === 'literal_bound' &&
          typeof step.value.value !== 'string')
      ) {
        return {
          ok: false,
          reason: 'contract_conflict',
          detail: `step ${i} has a malformed value binding`,
        }
      }
    }
    if (actionType === 'press' && !step.value) {
      return {
        ok: false,
        reason: 'contract_conflict',
        detail: `press step ${i} requires a bound key (slot or literal) — no key may be invented`,
      }
    }
    // Named-data validation: key membership counts (0 / false / '' are present);
    // a missing required slot stops the task BEFORE any browser mutation.
    if (step.value?.kind === 'slot') {
      const resolved = resolveDataSlotValue(step.value, requestData)
      if (!resolved.ok) {
        return {
          ok: false,
          reason: 'missing_data',
          detail: `missing required named data slot "${resolved.missingSlot}" (step ${i}) — validated before any mutation`,
        }
      }
      if (utf8Bytes(resolved.value) > FLOW_EXECUTOR_LIMITS.valueBytesMax) {
        return {
          ok: false,
          reason: 'contract_conflict',
          detail: `step ${i} bound value exceeds the ${FLOW_EXECUTOR_LIMITS.valueBytesMax}-byte limit`,
        }
      }
    } else if (step.value?.kind === 'literal_bound') {
      if (utf8Bytes(step.value.value) > FLOW_EXECUTOR_LIMITS.valueBytesMax) {
        return {
          ok: false,
          reason: 'contract_conflict',
          detail: `step ${i} literal value exceeds the ${FLOW_EXECUTOR_LIMITS.valueBytesMax}-byte limit`,
        }
      }
    }
    // P5 rework, finding 6: destination_screen_score thresholds are validated
    // BEFORE any browser mutation — an out-of-range or non-finite minScore is
    // a malformed postcondition and rejected here (no silent clamp).
    for (const pc of step.postconditions ?? []) {
      if (
        pc &&
        typeof pc === 'object' &&
        pc.kind === 'destination_screen_score' &&
        (typeof pc.minScore !== 'number' ||
          !Number.isFinite(pc.minScore) ||
          pc.minScore < 0 ||
          pc.minScore > 1)
      ) {
        return {
          ok: false,
          reason: 'contract_conflict',
          detail: `step ${i} destination_screen_score minScore must be a finite number within [0, 1]`,
        }
      }
    }
  }

  return { ok: true }
}

// ---------------------------------------------------------------------------
// Execution.
// ---------------------------------------------------------------------------

export interface FlowStepEventV01 {
  stepIndex: number
  action: IrActionKindV01
  category: string
  durationMs: number
  outcome: 'ok' | 'failed'
  bindingIdentity?: string
}

export interface FlowExecuteOptionsV01 {
  /** Task timeout budget (default `FLOW_EXECUTOR_LIMITS.defaultTaskTimeoutMs`). */
  timeoutMs?: number
  /** Secret data values to scrub from any surfaced note. */
  secrets?: string[]
  /** Injectable timer for deterministic tests. */
  sleep?: (ms: number) => Promise<void>
  /** Per-step telemetry hook (never receives secret data values). */
  onStep?: (evt: FlowStepEventV01) => void
  /** Persist a mutation checkpoint immediately before its deterministic dispatch begins. */
  onMutation?: (evt: { stepIndex: number; action: string }) => void | Promise<boolean | void>
  /** Hard cap on compiler-produced IR steps. */
  maxSteps?: number
  /** Hard cap on compiler-classified mutation steps. */
  maxMutations?: number
  /** Read-only mode rejects every compiler-classified mutation before binding. */
  mutationMode?: 'allow' | 'deny'
  /**
   * P5 — injectable postcondition evaluator (default: the real deterministic
   * evaluator in `lib/postconditions.ts`). Tests may inject a forced evaluator
   * to prove the real one is load-bearing (fake-green guard).
   */
  evaluateAssertions?: P5AssertionEvaluatorV01
  /**
   * P5 (rework round 3, findings 1+4) — field write-verification is
   * UNCONDITIONAL for every valued fill/type/select mutation: there is no
   * opt-out, no opt-in, and no flag. The round-2 escape hatch
   * (`verifyFieldWrites: false`) silently disabled ALL write verification and
   * is gone — a production contract can never green a write it did not
   * confirm. FAIL CLOSED: anything other than a definite page-level read-back —
   * a mismatch OR an unverifiable probe — fails the step BEFORE the next
   * mutation. Every dispatch shape carries its own identity-bound read-back:
   * a locator-dispatched fill re-reads the locator's node (verifyFieldWrite)
   * and a semantic-ref fill re-reads the SAME backendDOMNodeId the action rode
   * via the extension's identity-bound `readRefValue` verb (round 4, finding 1;
   * until that verb lands, a semantic-ref valued write is UNVERIFIABLE and
   * fails closed) — the live-ref runtime's transport ok is NEVER the write
   * proof (it runs Input.insertText without reading the value back). An
   * uploadFile succeeds only on the structured page operation's own
   * `{ ok: true }` attach confirmation. The gate keys on the ACTION KIND,
   * never on `step.mutation` (round 4, finding 2).
   */
  readonly noWriteVerificationBypassExists?: never
}

export interface FlowExecutionOutcomeV01 {
  result: BrowserTaskResultV01
  /** Request assertions were evaluated by the real P5 evaluator. */
  verification?: BrowserTaskVerification
  note?: string
}

/** Default sleep — real wall-clock timer. */
function defaultSleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

interface StepRunResultV01 {
  ok: boolean
  reason?: BrowserTaskFailureReasonV01
  detail?: string
  transientCode?: string
  /**
   * P5 (rework round 3, findings 1+4): true only when the valued mutation's
   * write was confirmed by a READ-BACK — the upload page code's own
   * `{ ok: true }` attach confirmation (dispatchVerb). The default write-check
   * gate keys on the dispatch category instead: a locator dispatch re-reads
   * the locator's node and a semantic-ref dispatch re-reads the SAME
   * backendDOMNodeId the action rode (`actedRefId`, via the extension's
   * identity-bound `readRefValue` verb) — the live-ref runtime's transport ok
   * is never a write proof (it runs Input.insertText without reading back).
   */
  writeVerified?: boolean
  /**
   * P5 rework round 4, finding 1: the backendDOMNodeId (refId) the action was
   * dispatched through, when the step rode a semantic-ref target. The
   * write-verification gate read-backs THAT node — identity-bound — instead of
   * re-selecting by role/index in DOM order.
   */
  actedRefId?: string
}

interface StepContextV01 {
  runtime: ExecutorRuntimeV01
  request: BrowserTaskRequestV01
  ir: BrowserFlowIRV01
  secrets: string[]
  sleep: (ms: number) => Promise<void>
  deadline: number
  recovery: { attempts: number; reasons: RecoveryReasonV01[] }
  /** P5 — injectable assertion evaluator (default: the real one). */
  evaluateAssertions: P5AssertionEvaluatorV01
}

/** Wrap a runtime op so a throwing seam becomes a structured failure. */
async function safeRuntime<T>(
  fn: () => Promise<T>
): Promise<{ ok: true; value: T } | { ok: false; error: string }> {
  try {
    return { ok: true, value: await fn() }
  } catch (err) {
    return { ok: false, error: err instanceof Error ? err.message : String(err) }
  }
}

function note(secrets: string[], text: string): string {
  return redactText(text, secrets)
}

/** Resolve a press key from the step's value binding (validation guarantees it). */
function resolvePressKey(
  step: FlowIRStepV01,
  requestData: Record<string, string | number | boolean> | undefined
): string {
  const resolved = resolveDataSlotValue(step.value, requestData)
  return resolved.ok ? resolved.value : ''
}

/** Send one extension verb with the exact args the bound runtime expects. */
async function dispatchVerb(
  ctx: StepContextV01,
  step: FlowIRStepV01,
  plan: ResolvedTargetV01,
  stepBudget: number
): Promise<StepRunResultV01> {
  const verb = plan.verb ?? step.action.type
  let args: Record<string, unknown>
  switch (verb) {
    case 'navigate':
      args = { url: plan.url ?? '' }
      break
    case 'click':
    case 'hover':
      args = { selector: plan.locator ?? '' }
      break
    case 'fill':
      args = { selector: plan.locator ?? '', value: plan.value ?? '' }
      break
    case 'type':
      args = { selector: plan.locator ?? '', text: plan.value ?? '' }
      break
    case 'select':
      args = { selector: plan.locator ?? '', value: plan.value ?? '' }
      break
    case 'press': {
      const key = resolvePressKey(step, ctx.request.data)
      if (!key) {
        return {
          ok: false,
          reason: 'step_failed',
          detail: `press step ${step.index} resolved to an empty key — no key may be invented`,
        }
      }
      args = plan.locator ? { key, selector: plan.locator } : { key }
      break
    }
    case 'uploadFile': {
      const fixture = getUploadFixture(plan.value ?? '')
      if (!fixture) {
        return {
          ok: false,
          reason: 'step_failed',
          detail: 'upload step ' + step.index + ' references an unknown fixture — no bytes were dispatched',
        }
      }
      args = {
        ...(plan.locator ? { selector: plan.locator } : {}),
        ...(Number.isSafeInteger(plan.semanticIndex) ? { semanticIndex: plan.semanticIndex } : {}),
        filename: fixture.filename,
        mimeType: fixture.mimeType,
        base64: fixture.base64,
      }
      break
    }
    default:
      return {
        ok: false,
        reason: 'step_failed',
        detail: `verb "${verb}" has no dispatch mapping for step ${step.index}`,
      }
  }
  const sent = await safeRuntime(() =>
    ctx.runtime.dispatch(verb, args, { timeoutMs: stepBudget })
  )
  if (!sent.ok) {
    return {
      ok: false,
      reason: 'error',
      detail: `dispatch to the bound runtime threw for step ${step.index}`,
    }
  }
  if (!sent.value.ok) {
    return {
      ok: false,
      reason: mapDispatchErrorCode(sent.value.error?.code),
      detail: extensionFailureDetail(step.index, verb, 'dispatch', sent.value.error?.code),
      transientCode: sent.value.error?.code,
    }
  }
  if (verb === 'uploadFile') {
    // Transport success ≠ upload success: the structured page operation reports its own
    // `{ ok, code? }` result. The extension forwards the structured result inside
    // `result.value`; anything but `ok:true`
    // fails the step closed.

    const raw = (sent.value.result ?? {}) as { value?: unknown }
    const pageResult = (raw.value ?? {}) as {
      ok?: boolean
      code?: string
      reason?: string
    }
    if (pageResult.ok !== true) {
      // B-14 re-review: the page-supplied `reason` can quote selectors,
      // values, and script errors — only the allowlisted code is diagnostic.
      return {
        ok: false,
        reason: 'step_failed',
        detail: `upload step ${step.index} failed in the page (${dispatchErrorCodeForLog(pageResult.code)}) — failed closed`,
      }
    }
    // The structured page operation's own `{ ok: true }` is the authoritative attach
    // confirmation; the write is verified by the
    // page itself and no locator probe can add evidence.
    return { ok: true, writeVerified: true }
  }
  return { ok: true }
}

/**
 * Execute ONE step against the bound runtime. Never mutates when the live
 * screen could not be synchronized + classified, when a broad locator has no
 * matching live semantic node, or when the IR step rejected at plan time.
 */
async function executeStep(
  ctx: StepContextV01,
  step: FlowIRStepV01,
  plan: ResolvedTargetV01,
  stepBudget: number
): Promise<StepRunResultV01> {
  const { runtime, request } = ctx

  switch (plan.kind) {
    case 'reject':
      return {
        ok: false,
        reason: 'step_failed',
        detail: plan.reason ?? `step ${step.index} rejected by the dispatch order`,
      }

    case 'wait': {
      const waitMs = Math.max(0, Math.min(plan.waitMs ?? 0, stepBudget))
      if (waitMs > 0) await ctx.sleep(waitMs)
      return { ok: true }
    }

    case 'assert': {
      // Bounded read: prove the current screen is readable. P5 owns condition
      // verification — this executor only guarantees a read happened.
      const snap = await safeRuntime(() => runtime.syncScreen())
      if (!snap.ok) {
        return {
          ok: false,
          reason: 'step_failed',
          detail: `assert step ${step.index} could not read the current screen`,
        }
      }
      return { ok: true }
    }

    case 'semantic_ref': {
      // Upload on a semantic file_field ref with no stored locator: the
      // uploadFile addresses the Nth file input by position and
      // reports NO_FILE_INPUT when nothing matches. performActionWithRef('fill')
      // would be a silent no-op on a file input, so the structured upload dispatches
      // directly — the structured upload operation is the authoritative existence check.
      if (plan.verb === 'uploadFile') {
        return dispatchVerb(ctx, step, plan, stepBudget)
      }
      const refs = await safeRuntime(() => runtime.snapshotRefs())
      if (!refs.ok) {
        return {
          ok: false,
          reason: 'error',
          detail: `live-ref snapshot threw before step ${step.index}`,
        }
      }
      // B-8: a recorded slot-template name that renders uniquely against this
      // run's data overrides the positional ref; otherwise (no template, no
      // unique name hit) bind exactly as before.
      const target =
        nameAnchoredLiveRef(
          refs.value,
          plan.semanticRole,
          step.action.type,
          step.target?.name,
          request.data
        ) ??
        findLiveRef(refs.value, plan.semanticRole, plan.semanticIndex, step.action.type)
      if (!target) {
        return {
          ok: false,
          reason: 'step_failed',
          // B-14 re-review: the semantic role is a free-form template string
          // and can carry page text — scrub it, keep the numeric index.
          detail: `no live semantic target for ${scrubSemanticRoleForLog(plan.semanticRole ?? 'element')}@${plan.semanticIndex ?? 0} — failed closed before mutating`,
        }
      }
      let args: unknown[]
      if (step.action.type === 'press') {
        args = [resolvePressKey(step, request.data)]
      } else if (VALUED_ACTION_SET.has(step.action.type)) {
        args = [plan.value ?? '']
      } else {
        args = []
      }
      const acted = await safeRuntime(() =>
        runtime.performActionWithRef(target.refId, step.action.type, args)
      )
      if (!acted.ok) {
        return {
          ok: false,
          reason: 'error',
          detail: `semantic action threw before step ${step.index}`,
        }
      }
      if (!acted.value.ok) {
        return {
          ok: false,
          reason: mapDispatchErrorCode(acted.value.error?.code),
          detail: extensionFailureDetail(step.index, step.action.type, 'perform', acted.value.error?.code),
          transientCode: acted.value.error?.code,
        }
      }
      if (VALUED_ACTION_SET.has(step.action.type)) {
        // P5 (rework round 3, finding 1): the live-ref runtime's ok is a
        // TRANSPORT confirmation only — it ran Input.insertText and never read
        // the field back (background.js). The write is verified separately by
        // an IDENTITY-BOUND read-back on THIS node's backendDOMNodeId after
        // the stabilized mutation (round 4, finding 1); see verifySemanticWrite.
        // The gate keys on `actedRefId` — never on a DOM-order re-selection.
        return { ok: true, actedRefId: target.refId }
      }
      return { ok: true }
    }

    case 'validated_locator': {
      // Structured upload: the page operation itself confirms the file input
      // exists (NO_FILE_INPUT when nothing matches). A file input's AX role is
      // not a FILLABLE role, so AX-role confirmation would fail closed on a
      // valid target — skip it for uploads only.
      if (plan.verb === 'uploadFile') {
        return dispatchVerb(ctx, step, plan, stepBudget)
      }
      // A broad cached selector must be semantically validated BEFORE mutation:
      // require a live node whose AX role matches the step's semantic role.
      const refs = await safeRuntime(() => runtime.snapshotRefs())
      if (!refs.ok) {
        return {
          ok: false,
          reason: 'error',
          detail: `live-ref snapshot threw before step ${step.index}`,
        }
      }
      const roles = acceptableRoles(plan.semanticRole, step.action.type)
      const confirmed = refs.value.some((n) => roles.has(n.role ?? ''))
      if (!confirmed) {
        return {
          ok: false,
          reason: 'step_failed',
          // B-14 re-review: the locator value is provably generic here (this
          // branch only runs for broad catch-all selectors per
          // `isBroadLocator`, which matches fixed tokens only), but the role
          // is free-form template text — scrub it.
          detail: `broad locator "${plan.locator ?? ''}" has no matching live ${scrubSemanticRoleForLog(plan.semanticRole ?? 'element')} node — failed closed before mutating`,
        }
      }
      return dispatchVerb(ctx, step, plan, stepBudget)
    }

    case 'dispatch':
      return dispatchVerb(ctx, step, plan, stepBudget)

    default:
      return {
        ok: false,
        reason: 'step_failed',
        detail: `unexecutable plan kind for step ${step.index}`,
      }
  }
}

/**
 * Run one step, applying the deterministic read-retry policy: mutations are
 * NEVER retried; read/idempotent steps may be retried only when the failure is
 * a transient transport code AND the IR recovery policy grants a bound.
 */
async function runStepWithRetry(
  ctx: StepContextV01,
  step: FlowIRStepV01,
  plan: ResolvedTargetV01,
  stepBudget: number
): Promise<StepRunResultV01> {
  const first = await executeStep(ctx, step, plan, stepBudget)
  if (first.ok || step.mutation) return first
  if (!isTransientCode(first.transientCode)) return first
  const budget = Math.min(
    readRetryBudget(ctx.ir.recoveryPolicy),
    FLOW_EXECUTOR_LIMITS.maxReadRetryAttempts
  )
  if (budget <= 0) return first
  let last = first
  let attempt = 0
  while (attempt < budget) {
    attempt++
    ctx.recovery.attempts++
    ctx.recovery.reasons.push('retry_read')
    await ctx.sleep(
      Math.min(FLOW_EXECUTOR_LIMITS.readRetryBackoffMs, Math.max(1, stepBudget))
    )
    const retried = await executeStep(ctx, step, plan, stepBudget)
    if (retried.ok) return retried
    last = retried
    if (!isTransientCode(retried.transientCode)) return retried
  }
  return last
}

/**
 * Bounded stabilization wait after a mutation: poll the live snapshot until
 * the URL/title is stable across two consecutive polls or the (longer, bounded)
 * budget for login/save/submit transitions is exhausted. Never polls
 * unboundedly; never fails the step — the mutation itself already succeeded.
 */
async function waitForStable(
  ctx: StepContextV01,
  step: FlowIRStepV01,
  remainingMs: number
): Promise<void> {
  const budget = Math.min(
    isSlowTransition(step)
      ? FLOW_EXECUTOR_LIMITS.slowTransitionStabilizeMs
      : FLOW_EXECUTOR_LIMITS.standardStabilizeMs,
    Math.max(0, remainingMs)
  )
  if (budget <= 0) return
  const deadline = ctx.runtime.now() + budget
  let prevKey: string | null = null
  let stableStreak = 0
  while (ctx.runtime.now() < deadline) {
    const snap = await safeRuntime(() => ctx.runtime.syncScreen())
    const key =
      snap.ok && snap.value
        ? `${snap.value.url ?? ''}\x00${snap.value.title ?? ''}`
        : null
    if (key !== null && key === prevKey) {
      stableStreak++
      if (stableStreak >= 2) return
    } else {
      stableStreak = 0
    }
    prevKey = key
    if (ctx.runtime.now() >= deadline) break
    await ctx.sleep(
      Math.min(FLOW_EXECUTOR_LIMITS.stabilizePollMs, Math.max(1, deadline - ctx.runtime.now()))
    )
  }
}

/**
 * Final-screen sync gated on the expected destination's route (B-15, OKE-43).
 *
 * After the last mutation, `waitForStable` certifies URL+title stability — but
 * a save roundtrip can outlast it, so the tab may still sit on the
 * pre-navigation page when the destination gate classifies. A fully stale
 * snapshot then elects the WRONG row with high confidence (warm 17/20 passed;
 * w10/w14/w18 failed as `destination_mismatch` while still on the add form).
 *
 * This gives the live page a bounded window to arrive at the expected
 * destination's route before that single-shot classification: a page already
 * on-route costs zero extra polls, and a page that never arrives fails closed
 * downstream with the SAME terminal as before. Reads only (`syncScreen`),
 * bounded by the remaining executor budget — total time never extends. The
 * convergence cap mirrors `waitForStable`'s own slow/standard judgment about
 * how long this flow's navigation may take.
 */
async function syncFinalScreenOnRoute(
  runtime: ExecutorRuntimeV01,
  sleep: (ms: number) => Promise<void>,
  deadline: number,
  lastStep: FlowIRStepV01 | undefined,
  expectedUrl: string | null | undefined
): Promise<{ ok: true; value: ResolverSnapshotV01 } | { ok: false; error: string }> {
  const first = await safeRuntime(() => runtime.syncScreen())
  if (!first.ok) return first
  const want = typeof expectedUrl === 'string' ? routeTemplate(expectedUrl) : null
  if (want === null) return first
  const convergeBudget = Math.min(
    lastStep !== undefined && isSlowTransition(lastStep)
      ? FLOW_EXECUTOR_LIMITS.slowTransitionStabilizeMs
      : FLOW_EXECUTOR_LIMITS.standardStabilizeMs,
    Math.max(0, deadline - runtime.now())
  )
  if (convergeBudget <= 0) return first
  const convergeDeadline = runtime.now() + convergeBudget
  let current: { ok: true; value: ResolverSnapshotV01 } | { ok: false; error: string } = first
  while (
    current.ok &&
    routeTemplate(current.value.url) !== want &&
    runtime.now() < convergeDeadline
  ) {
    await sleep(
      Math.min(FLOW_EXECUTOR_LIMITS.stabilizePollMs, Math.max(1, convergeDeadline - runtime.now()))
    )
    if (runtime.now() >= convergeDeadline) break
    current = await safeRuntime(() => runtime.syncScreen())
  }
  return current
}

// ---------------------------------------------------------------------------
// P5 — postcondition evaluation (deterministic, bounded, fail-closed).
// ---------------------------------------------------------------------------

interface PollOutcomeV01 {
  passed: boolean
  timedOut: boolean
  verdicts: readonly AssertionVerdictV01[]
}

/**
 * Collect ONE safe evidence bundle from the bound runtime. Never throws; a
 * failed collection yields `null` (the caller fails closed) or evidence with
 * availability flags set so a failed source can never masquerade as a passing
 * negative assertion.
 */
async function collectEvidence(
  ctx: StepContextV01,
  fieldProbes: Record<string, boolean> = {},
  rowKeys: string[] = [],
  rowScopes: { scope: string; key: string }[] | null = null
): Promise<PostconditionEvidenceV01 | null> {
  const snap = await safeRuntime(() => ctx.runtime.syncScreen())
  if (!snap.ok || !snap.value) return null
  let resolution: ScreenResolutionV01 | null = null
  try {
    resolution = ctx.runtime.classifyScreen(snap.value)
  } catch {
    resolution = null
  }
  const refs = await safeRuntime(() => ctx.runtime.snapshotRefs())
  const screen =
    resolution && resolution.status === 'resolved' && resolution.screen
      ? resolution.screen
      : null
  return {
    urlAvailable: true,
    url: snap.value.url ?? '',
    refsAvailable: refs.ok,
    refs: refs.ok ? refs.value : [],
    title: snap.value.title ?? null,
    headings: snap.value.headings ?? [],
    screen: screen
      ? {
          screenId: screen.screenId,
          routeFamily: screen.routeFamily ?? null,
          stateVariant: screen.stateVariant ?? null,
        }
      : null,
    screenConfidence: screen ? screen.confidence : null,
    modal: snap.value.modal ?? null,
    // P5 rework round 3, finding 2: syncScreen's optional modal channel — the
    // snapshot distinguishes "collected and no modal" (`modal: null`…actually
    // present as a collected channel, `open: false`/absent modal object) from
    // "channel not collected" (`modal: undefined`). Only a COLLECTED channel
    // may ground a `modal_state: closed` claim.
    modalAvailable: snap.value.modal !== undefined,
    tabPanel: snap.value.tabPanel ?? null,
    iframe: snap.value.iframe ?? null,
    statusText: [],
    rowKeys,
    // P5 rework round 3, finding 3: scope attribution for `row_exists`. null
    // when the scoped channel was not collected or could not be scoped — the
    // evaluator fails scoped assertions closed on it.
    rowScopes,
    fieldProbes,
    networkEvidence: null,
  }
}

/**
 * Poll checks against freshly collected evidence until every check passes, an
 * immediate negative is observed, an unsupported/error verdict appears (fail
 * closed at once — polling cannot change it), or the bounded budget is
 * exhausted. Reads only — a mutation is NEVER retried here.
 */
async function pollAssertionChecks(
  ctx: StepContextV01,
  checks: readonly AssertionCheckV01[],
  budgetMs: number,
  fieldProbes: Record<string, boolean> = {}
): Promise<PollOutcomeV01> {
  if (checks.length === 0) return { passed: true, timedOut: false, verdicts: [] }
  const deadline = ctx.runtime.now() + Math.max(0, budgetMs)
  let last: readonly AssertionVerdictV01[] = []
  let sawImmediateNegative = false
  let evidenceFailed = false
  const wantRowKeys = needsRowKeys(checks)
  const wantedScopes = neededRowScopes(checks)
  while (true) {
    const remaining = Math.max(1, deadline - ctx.runtime.now())
    // Harvest durable row/entity keys ONLY when a check needs them — the
    // extra rowKeys dispatch is skipped otherwise (no cost on ordinary
    // assertion polls). Fail-closed on any dispatch/shape error (empty keys,
    // null scopes). Round 3, finding 3: `row_exists` checks also need scope
    // attribution, collected by the same dispatch.
    const harvested =
      wantRowKeys || wantedScopes.length > 0
        ? await collectRowKeys(ctx, remaining, wantedScopes)
        : { rowKeys: [], rowScopes: null }
    const evidence = await collectEvidence(
      ctx,
      fieldProbes,
      harvested.rowKeys,
      harvested.rowScopes
    )
    if (!evidence) {
      evidenceFailed = true
      last = checks.map((c) => ({
        kind: 'error',
        operator: c.label,
        source: 'none',
        reason: 'live evidence could not be collected',
        definitive: false,
        timedOut: false,
        elapsedMs: ctx.runtime.now(),
      }))
      break
    }
    last = ctx.evaluateAssertions(checks, evidence, ctx.runtime.now())
    if (last.every((v) => v.kind === 'passed')) {
      return { passed: true, timedOut: false, verdicts: last }
    }
    if (last.some((v) => v.kind === 'unsupported' || v.kind === 'error')) break
    sawImmediateNegative =
      sawImmediateNegative || last.some((v) => v.kind === 'failed' && v.definitive)
    if (sawImmediateNegative) break
    if (ctx.runtime.now() >= deadline) break
    await ctx.sleep(
      Math.min(FLOW_EXECUTOR_LIMITS.assertionPollMs, Math.max(1, deadline - ctx.runtime.now()))
    )
  }
  const timedOut =
    !evidenceFailed &&
    !sawImmediateNegative &&
    !last.some((v) => v.kind === 'unsupported' || v.kind === 'error') &&
    last.some((v) => v.kind === 'failed')
  const verdicts = last.map((v) =>
    v.kind === 'failed' && timedOut ? { ...v, timedOut: true } : v
  )
  return { passed: false, timedOut, verdicts }
}

/**
 * Default field write-verification for fill/type/select that dispatch a
 * LOCATOR. Dispatches a boolean-only probe. FAIL CLOSED (P5 rework, finding 2):
 * the outcome is `'ok'` ONLY for a definite page-level confirmation; every
 * other case — definite mismatch, dispatch failure, missing locator/value, or
 * a page result that is not a boolean — returns `{ outcome, code }` with a
 * fixed safe code token and the step is failed BEFORE the next mutation.
 * Reads only; the mutation is never retried.
 *
 * SCOPE (rework round 3, findings 1+4): the write-verification gate keys on
 * the dispatch category, and every category has its own identity-bound probe —
 * none is ever opt-out. Locator-dispatched writes are verified here; a
 * semantic-ref dispatch has no CSS locator (dispatch-order never assigns one
 * to a semantic ref) and rides `verifySemanticWrite` instead, which re-reads
 * the SAME backendDOMNodeId the action rode via the extension's
 * identity-bound `readRefValue` verb (round 4, finding 1 — a DOM-order
 * role/index re-selection can never be a write proof). The live-ref runtime's
 * transport ok is never the write proof for either. An uploadFile
 * succeeds only on the structured page operation's own `{ ok: true }` outcome and is
 * exempted upstream.
 */
async function verifyFieldWrite(
  ctx: StepContextV01,
  step: FlowIRStepV01,
  plan: ResolvedTargetV01,
  budgetMs: number
): Promise<'ok' | { outcome: 'mismatch' | 'unverifiable'; code: string }> {
  const locator = plan.locator
  const value = plan.value
  if (!locator) return { outcome: 'unverifiable', code: 'PROBE_NO_LOCATOR' }
  if (value === undefined) return { outcome: 'unverifiable', code: 'PROBE_NO_VALUE' }
  if (value === '') return { outcome: 'unverifiable', code: 'PROBE_EMPTY_VALUE' }
  const sent = await safeRuntime(() =>
    ctx.runtime.dispatch(
      'selectorValue',
      {
        selector: locator,
        expected: value,
        match: step.action.type === 'type' ? 'contains' : 'equals',
        action: step.action.type,
      },
      {
        timeoutMs: Math.min(
          FLOW_EXECUTOR_LIMITS.standardStabilizeMs,
          Math.max(1, budgetMs)
        ),
      }
    )
  )
  if (!sent.ok) return { outcome: 'unverifiable', code: 'PROBE_DISPATCH_FAILED' }
  const probe = interpretFieldProbe(sent.value)
  if (probe === 'ok') return 'ok'
  if (probe === 'mismatch') return { outcome: 'mismatch', code: 'PROBE_VALUE_MISMATCH' }
  return { outcome: 'unverifiable', code: 'PROBE_NO_PAGE_RESULT' }
}

/**
 * P5 (rework round 3, finding 1) — write-verification probe for a SEMANTIC-REF
 * dispatch. The live-ref runtime resolved the target by (role, index) over the
 * AX snapshot and dispatched `Input.insertText`; its `{ ok: true }` confirms
 * the transport only — it never read the field value back.
 *
 * P5 rework round 4, finding 1 (CRITICAL): the round-3 probe re-selected the
 * target via `document.querySelectorAll()` in DOM order and compared the
 * index-th candidate — NOT the node the action actually touched. The
 * reviewer's empirical probe showed a hidden `aria-hidden` clone field with a
 * pre-existing value passing such a probe while the real node stayed empty
 * (`axTargetValue:''`, `domProbeTarget:'hidden aria-hidden'`,
 * `probeResult:{ok:true}`). A DOM-order re-selection can never be a write
 * proof: identity is only guaranteed by reading back the SAME
 * backendDOMNodeId the action used.
 *
 * The fix: the executor dispatches the identity-bound verb
 * `readRefValue` ({refId, expect, match}); the extension resolves the SAME
 * node from its own ref registry (bannerless-v1 `dom:<generation>:<index>`
 * refs, round 5: background.js router case + bannerless-page `readRefValue`
 * operation) and compares the value on THAT node. The live router flattens
 * the boolean-only page verdict into
 * `{ok:true, result:{refId, verified:boolean, match}}` — never echoing the
 * value (canonical envelope, locked by the extension's read-ref-value router
 * test and interpreted by `interpretSemanticRefValueProbe`; the legacy
 * pre-flatten `{ok:true, result:{value:{ok, code}}}` page result stays
 * accepted). If the extension ever rejects the verb at the transport level
 * (`ok:false`), `verifySemanticWrite` fails closed with
 * PROBE_NO_IDENTITY_VERB — an unverifiable write must not read as verified.
 * `fill`/`select` compare with equals, `type` with contains; an empty
 * expectation is refused pre-dispatch (PROBE_EMPTY_VALUE, round 5 finding 4).
 *
 * P5 rework round 6 (action-awareness): the dispatch payload is unchanged,
 * but the read-back itself became CONTROL-AWARE in the page op — a
 * checked-state control compares `checked` (expect "true"/"false" only),
 * and a SELECT accepts the selected option's value OR its label (the same
 * two shapes `select` action matches on). Additionally, no valued write can
 * ever reach a checked-state control: `acceptableRoles` strips
 * checkbox/radio/switch for valued actions, and the page op's act() rejects
 * fill/type on checked-type inputs — defense in depth on both sides of the
 * transport.
 */

/**
 * Dispatch the identity-bound read-back and map it onto the fail-closed
 * outcome. Reads only — never mutates, never retries the mutation.
 * `refId` is the backendDOMNodeId the action was dispatched through
 * (`StepRunResultV01.actedRefId`); without it the write is unverifiable.
 */
async function verifySemanticWrite(
  ctx: StepContextV01,
  step: FlowIRStepV01,
  plan: ResolvedTargetV01,
  budgetMs: number,
  refId: string | undefined
): Promise<'ok' | { outcome: 'mismatch' | 'unverifiable'; code: string }> {
  const value = plan.value
  if (value === undefined) return { outcome: 'unverifiable', code: 'PROBE_NO_VALUE' }
  // P5 rework round 5, finding 4: a `match: 'contains'` probe with an empty
  // expectation is VACUOUS (''.includes('') is true on every node) — it would
  // fake a confirmed write. The locator path already guards this
  // (PROBE_EMPTY_VALUE); the semantic path enforces the same bound before the
  // dispatch ever happens.
  if (value === '') return { outcome: 'unverifiable', code: 'PROBE_EMPTY_VALUE' }
  // P5 rework round 4, finding 1: identity is REQUIRED. A missing refId means
  // the step never rode a semantic-ref target (or a future caller forgot to
  // thread it) — the read-back cannot bind to a node, so the write is
  // unverifiable. Never fall back to a role/index DOM re-selection.
  if (!refId) return { outcome: 'unverifiable', code: 'PROBE_NO_IDENTITY_VERB' }
  const sent = await safeRuntime(() =>
    ctx.runtime.dispatch(
      SEMANTIC_REF_PROBE_VERB,
      {
        refId,
        expect: value,
        match: step.action.type === 'type' ? 'contains' : 'equals',
      },
      {
        timeoutMs: Math.min(
          FLOW_EXECUTOR_LIMITS.standardStabilizeMs,
          Math.max(1, budgetMs)
        ),
      }
    )
  )
  if (!sent.ok) return { outcome: 'unverifiable', code: 'PROBE_DISPATCH_FAILED' }
  // A resolved `{ok:false}` envelope is a transport-level rejection of the
  // read-back itself (e.g. the ref is no longer in the extension's ref
  // cache). Nothing about the page was learned — that is NOT a mismatch; it is
  // unverifiable, and the gate fails closed.
  if (!sent.value.ok) return { outcome: 'unverifiable', code: 'PROBE_NO_IDENTITY_VERB' }
  const probe = interpretSemanticRefValueProbe(sent.value)
  if (probe === 'ok') return 'ok'
  if (probe === 'mismatch') return { outcome: 'mismatch', code: 'PROBE_VALUE_MISMATCH' }
  return { outcome: 'unverifiable', code: 'PROBE_NO_PAGE_RESULT' }
}

/** Does any check require durable row/entity evidence (`record_id`)? */
function needsRowKeys(checks: readonly AssertionCheckV01[]): boolean {
  return checks.some((c) => c.operator.kind === 'record_id')
}

/** Distinct `row_exists` scope tokens, in first-seen order (round 3, finding 3). */
function neededRowScopes(checks: readonly AssertionCheckV01[]): string[] {
  const out: string[] = []
  const seen = new Set<string>()
  for (const c of checks) {
    const op = c.operator
    if (op.kind !== 'row_exists') continue
    if (typeof op.scope !== 'string') continue
    const s = op.scope.trim()
    if (!s || seen.has(s)) continue
    seen.add(s)
    out.push(s)
  }
  return out
}

/**
 * Harvest durable row/entity keys from the bound document via ONE read-only
 * structured read dispatch (the `verifyFieldWrite` precedent). Reads only — no DOM
 * mutation, no input values, no retry of any action. Fail closed (P5 rework
 * round 2, finding 6): a transport error or a malformed page result yields
 * `[]`, so a `record_id` assertion with no positively-evidenced row key fails
 * closed instead of guessing from url shape.
 *
 * Round 3, finding 3: with non-empty `scopes` the SAME dispatch also harvests
 * scope-attributed rows for `row_exists`. `rowScopes` is `null` when the
 * channel is unscoped, uncollectable, or a requested scope was not
 * representable — the evaluator fails scoped assertions closed on it.
 */
async function collectRowKeys(
  ctx: StepContextV01,
  budgetMs: number,
  scopes: readonly string[] = []
): Promise<{ rowKeys: string[]; rowScopes: { scope: string; key: string }[] | null }> {
  const sent = await safeRuntime(() =>
    ctx.runtime.dispatch(
      'rowKeys',
      { scopes: [...scopes] },
      {
        timeoutMs: Math.min(
          FLOW_EXECUTOR_LIMITS.standardStabilizeMs,
          Math.max(1, budgetMs)
        ),
      }
    )
  )
  if (!sent.ok) return { rowKeys: [], rowScopes: null }
  const rowKeys = interpretRowKeyProbe(sent.value) ?? []
  const rowScopes = scopes.length > 0 ? interpretRowKeyProbeWithScopes(sent.value) : null
  return { rowKeys, rowScopes }
}

/** Evaluate a step's postconditions against a bounded poll of live evidence. */
async function evaluateStepPostconditions(
  ctx: StepContextV01,
  step: FlowIRStepV01,
  budgetMs: number
): Promise<PollOutcomeV01> {
  const checks: AssertionCheckV01[] = (step.postconditions ?? []).map((pc, idx) => {
    const { operator, label } = resolveStepPostcondition(pc, ctx.request)
    return { index: idx, label, operator }
  })
  return pollAssertionChecks(ctx, checks, budgetMs)
}

/** Evaluate the request's final business assertions against live evidence. */
async function evaluateFinalAssertions(
  ctx: StepContextV01,
  budgetMs: number
): Promise<PollOutcomeV01> {
  const assertions = ctx.request.assertions ?? []
  const checks: AssertionCheckV01[] = assertions.map((a, idx) => ({
    index: idx,
    label: (a && typeof a === 'object' && (a as { kind?: string }).kind) || 'assertion',
    operator: operatorFromTaskAssertion(a),
  }))
  return pollAssertionChecks(ctx, checks, budgetMs)
}

/**
 * Build the terminal result for a failed postcondition/assertion evaluation.
 * An `unsupported` (or `error`) verdict maps to the frozen canonical
 * `contract_conflict` reason — P5 never widens the frozen failure union — and a
 * genuinely failed assertion maps to `step_failed`. The note is a compact,
 * redacted summary (operator kind, verdict, safe source, reason token, timing).
 */
function postconditionTerminal(
  ctx: StepContextV01,
  ir: BrowserFlowIRV01,
  outcome: PollOutcomeV01,
  stepsRun: number,
  path: ExecutionPathV01[],
  recovery: { attempts: number; reasons: RecoveryReasonV01[] },
  bindingIdentity: string | undefined,
  startedAt: number,
  request: BrowserTaskRequestV01,
  destination: ScreenIdentityV01 | undefined,
  destinationScore: number | undefined,
  where: 'step' | 'final'
): FlowExecutionOutcomeV01 {
  const unsupportedOrError = outcome.verdicts.some(
    (v) => v.kind === 'unsupported' || v.kind === 'error'
  )
  const reason: BrowserTaskFailureReasonV01 = unsupportedOrError
    ? 'contract_conflict'
    : 'step_failed'
  const summary = summarizeAssertions(outcome.verdicts)
  const timing = outcome.timedOut ? ' (assertion budget exhausted)' : ''
  const failClosed = unsupportedOrError
    ? ' — an unsupported assertion failed closed'
    : ''
  const text =
    where === 'step'
      ? `step postcondition verification failed: ${summary}${timing}${failClosed}`
      : `final business assertion failed: ${summary}${timing}${failClosed}`
  return buildTerminal({
    request,
    ir,
    stepsRun,
    status: statusForReason(reason),
    failureReason: reason,
    ...(destination ? { destination } : {}),
    ...(destinationScore !== undefined ? { destinationScore } : {}),
    path,
    recovery,
    bindingIdentity,
    startedAt,
    nowMs: ctx.runtime.now(),
    ...(where === 'final'
      ? {
          verification: {
            status: 'failed' as const,
            assertionsPassed: outcome.verdicts.filter((verdict) => verdict.kind === 'passed').length,
            assertionsTotal: request.assertions?.length ?? 0,
          },
        }
      : {}),
    note: note(ctx.secrets, text),
  })
}

interface TerminalBuildOptions {
  request: BrowserTaskRequestV01
  ir: BrowserFlowIRV01
  stepsRun: number
  status: BrowserTaskResultV01['status']
  failureReason?: BrowserTaskFailureReasonV01
  destination?: ScreenIdentityV01
  destinationScore?: number
  recovery?: { attempts: number; reasons: RecoveryReasonV01[] }
  path: ExecutionPathV01[]
  bindingIdentity?: string
  startedAt: number
  nowMs: number
  verification?: BrowserTaskVerification
  note?: string
}

/** Build a terminal (0 remaining steps) frozen §3 result. */
function buildTerminal(opts: TerminalBuildOptions): FlowExecutionOutcomeV01 {
  const elapsed = Math.max(0, opts.nowMs - opts.startedAt)
  return {
    result: {
      version: '0.1',
      requestId: opts.request.requestId,
      status: opts.status,
      stepsRun: opts.stepsRun,
      stepsTotal: opts.ir.steps.length,
      ...(opts.destination ? { destination: opts.destination } : {}),
      ...(opts.destinationScore !== undefined
        ? { destinationScore: opts.destinationScore }
        : {}),
      ...(opts.failureReason ? { failureReason: opts.failureReason } : {}),
      ...(opts.recovery && opts.recovery.attempts > 0 ? { recovery: opts.recovery } : {}),
      metrics: {
        coreDurationMs: elapsed,
        wallDurationMs: elapsed,
        modelApiCalls: 0,
        cacheHits: 0,
        path: opts.path,
        ...(opts.bindingIdentity ? { browserTabId: opts.bindingIdentity } : {}),
      },
    },
    ...(opts.verification ? { verification: opts.verification } : {}),
    ...(opts.note ? { note: opts.note } : {}),
  }
}

function finitePositiveOr(
  fallback: number,
  v: number | undefined
): number {
  return typeof v === 'number' && Number.isFinite(v) && v > 0 ? v : fallback
}

/**
 * Execute a validated IR against one bound runtime. ALWAYS resolves to a
 * terminal outcome — never rejects, never yields a partial result.
 *
 * Ordering guarantees (deterministic): acquire once → per step in IR order
 * [reject before dispatch, binding-drift check before mutation, screen
 * sync+classify before mutation, start-screen match on step 0, bounded dispatch,
 * read-only retry only on transient transport codes, stabilization after
 * mutation] → stop immediately on the first failure (later mutations never
 * execute) → destination gate + assertions gate before any green.
 */
export async function executeFlow(
  request: BrowserTaskRequestV01,
  ir: BrowserFlowIRV01,
  runtime: ExecutorRuntimeV01,
  options: FlowExecuteOptionsV01 = {}
): Promise<FlowExecutionOutcomeV01> {
  const secrets = options.secrets ?? []
  const sleep = options.sleep ?? defaultSleep
  const startedAt = runtime.now()
  const taskTimeout = finitePositiveOr(
    FLOW_EXECUTOR_LIMITS.defaultTaskTimeoutMs,
    options.timeoutMs ?? request.timeoutMs
  )
  const deadline = startedAt + taskTimeout
  const recovery = { attempts: 0, reasons: [] as RecoveryReasonV01[] }
  const path: ExecutionPathV01[] = []

  const ctx: StepContextV01 = {
    runtime,
    request,
    ir,
    secrets,
    sleep,
    deadline,
    recovery,
    evaluateAssertions: options.evaluateAssertions ?? defaultP5Evaluator,
  }
  // P5 (rework round 3, finding 4): field write-verification is UNCONDITIONAL
  // for every valued fill/type/select mutation. The round-2 `verifyFieldWrites`
  // escape hatch is gone — a bypass that silently disabled ALL write
  // verification cannot live in the production contract. Every dispatch shape
  // carries its own identity-bound read-back (see the gate below).

  try {
    // ---- P5 rework round 5, finding 2: validate the IR BEFORE binding or the
    // first dispatch. The production runner already validates at its entry
    // (browser-task-runner.ts:1453), but `executeFlow` is exported — a direct
    // caller handing it malformed IR (e.g. a valued fill marked
    // `mutation:false`) would dispatch the mutation before any failure. The
    // contract must be closed at the executor itself: reject first, bind
    // nothing, dispatch nothing. ----
    const irCheck = validateFlowIr(ir, request.data)
    if (!irCheck.ok) {
      return buildTerminal({
        request,
        ir,
        stepsRun: 0,
        status: statusForReason(irCheck.reason),
        failureReason: irCheck.reason,
        path,
        recovery,
        startedAt,
        nowMs: runtime.now(),
        note: note(
          secrets,
          `${irCheck.detail} — nothing was executed and nothing was mutated`
        ),
      })
    }

    const maxSteps = options.maxSteps
    if (
      maxSteps !== undefined &&
      (!Number.isSafeInteger(maxSteps) || maxSteps < 1 || ir.steps.length > maxSteps)
    ) {
      return buildTerminal({
        request,
        ir,
        stepsRun: 0,
        status: 'contract_conflict',
        failureReason: 'contract_conflict',
        path,
        recovery,
        startedAt,
        nowMs: runtime.now(),
        note: note(
          secrets,
          `execution step budget exceeded (${ir.steps.length} steps; maximum ${String(maxSteps)}) — nothing was executed and nothing was mutated`,
        ),
      })
    }
    const mutationCount = ir.steps.reduce((count, step) => count + (step.mutation ? 1 : 0), 0)
    const mutationMode = options.mutationMode ?? 'allow'
    if (mutationMode !== 'allow' && mutationMode !== 'deny') {
      return buildTerminal({
        request,
        ir,
        stepsRun: 0,
        status: 'contract_conflict',
        failureReason: 'contract_conflict',
        path,
        recovery,
        startedAt,
        nowMs: runtime.now(),
        note: note(secrets, 'invalid mutation mode — nothing was executed and nothing was mutated'),
      })
    }
    if (mutationMode === 'deny' && mutationCount > 0) {
      return buildTerminal({
        request,
        ir,
        stepsRun: 0,
        status: 'contract_conflict',
        failureReason: 'contract_conflict',
        path,
        recovery,
        startedAt,
        nowMs: runtime.now(),
        note: note(secrets, `read-only mutation mode rejected ${mutationCount} planned mutation(s) — nothing was executed`),
      })
    }
    const maxMutations = options.maxMutations
    if (
      maxMutations !== undefined &&
      (!Number.isSafeInteger(maxMutations) || maxMutations < 0 || mutationCount > maxMutations)
    ) {
      return buildTerminal({
        request,
        ir,
        stepsRun: 0,
        status: 'contract_conflict',
        failureReason: 'contract_conflict',
        path,
        recovery,
        startedAt,
        nowMs: runtime.now(),
        note: note(
          secrets,
          `mutation budget exceeded (${mutationCount} planned mutation(s); maximum ${String(maxMutations)}) — nothing was executed`,
        ),
      })
    }

    // ---- bind exactly ONE session/tab/lease for the entire task. ----
    const acquired = await safeRuntime(() => runtime.acquire())
    if (!acquired.ok || !acquired.value.ok) {
      const reason: RuntimeAcquireReasonV01 = !acquired.ok
        ? 'error'
        : acquired.value.reason ?? 'error'
      return buildTerminal({
        request,
        ir,
        stepsRun: 0,
        status: statusForReason(reason),
        failureReason: reason,
        path,
        recovery,
        startedAt,
        nowMs: runtime.now(),
        note: note(
          secrets,
          `browser runtime could not be bound (${reason}) — nothing was executed and nothing was mutated`
        ),
      })
    }
    const bindingIdentity = acquired.value.binding?.identityKey
      ? String(acquired.value.binding.identityKey)
      : acquired.value.binding?.tabId !== undefined
        ? String(acquired.value.binding.tabId)
        : undefined

    // ---- step loop, in IR order. ----
    let stepsRun = 0
    for (let i = 0; i < ir.steps.length; i++) {
      const step = ir.steps[i]
      const stepStart = runtime.now()
      const remaining = deadline - runtime.now()
      if (remaining <= 0) {
        return buildTerminal({
          request,
          ir,
          stepsRun,
          status: 'error',
          failureReason: 'error',
          path,
          recovery,
          bindingIdentity,
          startedAt,
          nowMs: runtime.now(),
          note: note(
            secrets,
            `task timeout budget exhausted before step ${i} — stopped without executing it`
          ),
        })
      }
      const stepBudget = Math.max(0, Math.min(step.timeoutMs, remaining))

      // Plan the target decision FIRST; an unrunnable step stops the flow.
      const plan = planTargetResolution(step, request.data)
      if (plan.kind === 'reject') {
        options.onStep?.({
          stepIndex: i,
          action: step.action.type,
          category: plan.category,
          durationMs: runtime.now() - stepStart,
          outcome: 'failed',
          bindingIdentity,
        })
        return buildTerminal({
          request,
          ir,
          stepsRun,
          status: statusForReason('step_failed'),
          failureReason: 'step_failed',
          path,
          recovery,
          bindingIdentity,
          startedAt,
          nowMs: runtime.now(),
          note: note(
            secrets,
            `step ${i} rejected: ${plan.reason ?? 'no supported dispatch'} — nothing was mutated`
          ),
        })
      }

      // ---- before EVERY mutation: binding drift check + live screen
      // sync + classify. Failure here fails closed (no mutation). ----
      if (step.mutation) {
        const bound = await safeRuntime(() => runtime.verifyBinding())
        if (!bound.ok || bound.value !== true) {
          options.onStep?.({
            stepIndex: i,
            action: step.action.type,
            category: plan.category,
            durationMs: runtime.now() - stepStart,
            outcome: 'failed',
            bindingIdentity,
          })
          return buildTerminal({
            request,
            ir,
            stepsRun,
            status: statusForReason('error'),
            failureReason: 'error',
            path,
            recovery,
            bindingIdentity,
            startedAt,
            nowMs: runtime.now(),
            note: note(
              secrets,
              `runtime binding drifted before step ${i} (${step.action.type}) — failed closed before any mutation`
            ),
          })
        }
        const sync = await safeRuntime(() => runtime.syncScreen())
        if (!sync.ok) {
          options.onStep?.({
            stepIndex: i,
            action: step.action.type,
            category: plan.category,
            durationMs: runtime.now() - stepStart,
            outcome: 'failed',
            bindingIdentity,
          })
          return buildTerminal({
            request,
            ir,
            stepsRun,
            status: statusForReason('error'),
            failureReason: 'error',
            path,
            recovery,
            bindingIdentity,
            startedAt,
            nowMs: runtime.now(),
            note: note(
              secrets,
              `could not synchronize the current screen before step ${i} — failed closed before any mutation`
            ),
          })
        }
        let resolution = runtime.classifyScreen(sync.value)
        // Warm-benchmark finding B-3: the draft-local replay pool mints two
        // identities for pre/post content variants of ONE page (an unfiltered
        // customer list vs the same list after a client-side filter differ only
        // in row COUNT, hence signature) and they tie inside the ambiguity band.
        // Because the executor classifies before EVERY mutation, that twin tie
        // fails the replay closed at step 0 before the first action. When the
        // LIVE screen's origin+path is provably one of the episode's OWN endpoint
        // paths we are demonstrably on the right page (not a drifted one), so
        // resolve to the start identity by URL and keep going; the P5 assertion
        // gate at the destination still owns the business verdict, so a wrong
        // page cascades to a failed assertion rather than a false green. Any
        // other reason, or a path outside the episode's endpoints, fails closed.
        if (resolution.status !== 'resolved' || !resolution.screen) {
          const startRoute = routeTemplate(ir.startScreen?.url)
          const finalRoute = routeTemplate(ir.expectedFinalScreen?.url)
          const episodeRoutes = new Set<string>()
          if (startRoute) episodeRoutes.add(startRoute)
          if (finalRoute) episodeRoutes.add(finalRoute)
          if (
            episodeRoutes.size > 0 &&
            benignTwinAmbiguity(resolution, sync.value.url, episodeRoutes)
          ) {
            resolution = {
              status: 'resolved',
              screen: ir.startScreen ?? ir.expectedFinalScreen,
            } as typeof resolution
          }
        }
        if (resolution.status !== 'resolved' || !resolution.screen) {
          const reason = resolution.reason ?? 'screen_unknown'
          options.onStep?.({
            stepIndex: i,
            action: step.action.type,
            category: plan.category,
            durationMs: runtime.now() - stepStart,
            outcome: 'failed',
            bindingIdentity,
          })
          return buildTerminal({
            request,
            ir,
            stepsRun,
            status: statusForReason(reason),
            failureReason: reason,
            path,
            recovery,
            bindingIdentity,
            startedAt,
            nowMs: runtime.now(),
            note: note(
              secrets,
              `current screen could not be classified before step ${i} (${reason}; liveRoute=${
                routeTemplate(sync.value.url) ?? 'n/a'
              } episode=[${[
                routeTemplate(ir.startScreen?.url),
                routeTemplate(ir.expectedFinalScreen?.url),
              ]
                .filter(Boolean)
                .join(',') ?? 'none'}]) — failed closed before any mutation`
            ),
          })
        }
        // Step 0: confirm we are on the IR's start screen (when a numeric id
        // is known) so the first mutation cannot land on a drifted page.
        if (i === 0 && ir.startScreen && ir.startScreen.screenId > 0) {
          if (resolution.screen.screenId !== ir.startScreen.screenId) {
            options.onStep?.({
              stepIndex: i,
              action: step.action.type,
              category: plan.category,
              durationMs: runtime.now() - stepStart,
              outcome: 'failed',
              bindingIdentity,
            })
            return buildTerminal({
              request,
              ir,
              stepsRun,
              status: statusForReason('screen_stale'),
              failureReason: 'screen_stale',
              path,
              recovery,
              bindingIdentity,
              startedAt,
              nowMs: runtime.now(),
              note: note(
                secrets,
                `current screen (${resolution.screen.screenId}) does not match the IR start screen (${ir.startScreen.screenId}) — failed closed before the first mutation`
              ),
            })
          }
        }
      }

      // ---- execute the planned step (with bounded read retry only). ----
      if (step.mutation && options.onMutation) {
        const checkpoint = await options.onMutation({ stepIndex: i, action: step.action.type })
        if (checkpoint === false) {
          options.onStep?.({
            stepIndex: i,
            action: step.action.type,
            category: plan.category,
            durationMs: runtime.now() - stepStart,
            outcome: 'failed',
            bindingIdentity,
          })
          return buildTerminal({
            request,
            ir,
            stepsRun,
            status: statusForReason('error'),
            failureReason: 'error',
            path,
            recovery,
            bindingIdentity,
            startedAt,
            nowMs: runtime.now(),
            note: note(secrets, 'durable mutation checkpoint failed before dispatch — failed closed before dispatch'),
          })
        }
      }
      const outcome = await runStepWithRetry(ctx, step, plan, stepBudget)
      if (!outcome.ok) {
        options.onStep?.({
          stepIndex: i,
          action: step.action.type,
          category: plan.category,
          durationMs: runtime.now() - stepStart,
          outcome: 'failed',
          bindingIdentity,
        })
        return buildTerminal({
          request,
          ir,
          stepsRun,
          status: statusForReason(outcome.reason ?? 'step_failed'),
          failureReason: outcome.reason ?? 'step_failed',
          path,
          recovery,
          bindingIdentity,
          startedAt,
          nowMs: runtime.now(),
          note: note(
            secrets,
            `step ${i} (${step.action.type}) failed: ${outcome.detail ?? 'unknown error'} — stopped; later mutations were never executed`
          ),
        })
      }

      path.push(pathTokenFor(plan))
      options.onStep?.({
        stepIndex: i,
        action: step.action.type,
        category: plan.category,
        durationMs: runtime.now() - stepStart,
        outcome: 'ok',
        bindingIdentity,
      })
      stepsRun = i + 1

      // ---- bounded stabilization after every mutation. ----
      if (step.mutation) {
        await waitForStable(ctx, step, Math.max(0, deadline - runtime.now()))
      }

      // ---- P5: field write-verification for every valued mutation —
      // UNCONDITIONAL (round 3, finding 4: no bypass exists). FAIL CLOSED
      // (rounds 2+3): anything other than a definite page-level read-back —
      // mismatch OR unverifiable — fails the step BEFORE the next mutation.
      // Reads only; the mutation is never retried. The probe is identity-bound
      // per dispatch category: a locator dispatch re-reads the locator's node;
      // a semantic-ref dispatch re-reads the SAME backendDOMNodeId the action
      // rode (`outcome.actedRefId`, via the extension's identity-bound
      // `readRefValue` verb — round 4, finding 1; never a DOM-order
      // re-selection); an uploadFile carries its own structured page
      // outcome (writeVerified).
      // P5 rework round 4, finding 2: the gate keys on the ACTION KIND, never
      // on `step.mutation` — the flag is the retry-policy idempotency
      // designation and the round-4 probe showed a flag-crafted bypass.
      // validateFlowIr now rejects a non-mutating valued-write step, and the
      // kind-keyed condition here is the second line of defense. ----
      if (
        !outcome.writeVerified &&
        VALUED_WRITE_KINDS.has(step.action.type)
      ) {
        const probe =
          plan.category === 'semantic_ref'
            ? await verifySemanticWrite(
                ctx,
                step,
                plan,
                Math.max(0, deadline - runtime.now()),
                outcome.actedRefId
              )
            : await verifyFieldWrite(ctx, step, plan, Math.max(0, deadline - runtime.now()))
        if (probe !== 'ok') {
          options.onStep?.({
            stepIndex: i,
            action: step.action.type,
            category: plan.category,
            durationMs: runtime.now() - stepStart,
            outcome: 'failed',
            bindingIdentity,
          })
          const probeNote =
            probe.outcome === 'mismatch'
              ? 'field write-verification failed: the field does not hold the dispatched value'
              : `field write-verification could not confirm the dispatched value (${probe.code}) — failed closed before the next mutation`
          return buildTerminal({
            request,
            ir,
            stepsRun,
            status: statusForReason('step_failed'),
            failureReason: 'step_failed',
            path,
            recovery,
            bindingIdentity,
            startedAt,
            nowMs: runtime.now(),
            note: note(secrets, probeNote),
          })
        }
      }

      // ---- P5: evaluate this step's postconditions before the next mutation.
      // Reads only, bounded by the (longer, bounded) stabilization budget for
      // slow transitions; an unsupported shape fails closed immediately. ----
      if (step.postconditions && step.postconditions.length > 0) {
        const postOutcome = await evaluateStepPostconditions(
          ctx,
          step,
          Math.max(0, deadline - runtime.now())
        )
        if (!postOutcome.passed) {
          return postconditionTerminal(
            ctx,
            ir,
            postOutcome,
            stepsRun,
            path,
            recovery,
            bindingIdentity,
            startedAt,
            request,
            undefined,
            undefined,
            'step'
          )
        }
      }
    }

    // ---- destination gate: green requires the final live screen to match
    // the IR's expected business destination. ----
    if (!ir.expectedFinalScreen) {
      return buildTerminal({
        request,
        ir,
        stepsRun,
        status: statusForReason('no_expected_destination'),
        failureReason: 'no_expected_destination',
        path,
        recovery,
        bindingIdentity,
        startedAt,
        nowMs: runtime.now(),
        note: note(
          secrets,
          `executed all ${ir.steps.length} steps but the IR records no expected destination screen — no green claimed (P5 owns the final verdict)`
        ),
      })
    }
    const finalSync = await syncFinalScreenOnRoute(
      runtime,
      sleep,
      deadline,
      ir.steps.length > 0 ? ir.steps[ir.steps.length - 1] : undefined,
      ir.expectedFinalScreen?.url
    )
    if (!finalSync.ok) {
      return buildTerminal({
        request,
        ir,
        stepsRun,
        status: statusForReason('error'),
        failureReason: 'error',
        path,
        recovery,
        bindingIdentity,
        startedAt,
        nowMs: runtime.now(),
        note: note(secrets, `could not synchronize the final screen after all steps`),
      })
    }
    const finalRes = runtime.classifyScreen(finalSync.value)
    // ---- Destination-gate drift tolerance (warm-benchmark finding B-1). ----
    // `screen_ambiguous` is NOT "we are on the wrong screen" — it means the
    // resolver could not elect ONE row among near-identical candidates. The
    // common benign case: a list and the SAME list after a client-side filter
    // (a token of `formFieldTypes`/headings diverged while the document title
    // stayed equal), which the draft pool mints as two identities that then tie
    // inside the ambiguity band. Those twins are the same place. When the LIVE
    // screen's origin+path is provably the expected destination's origin+path,
    // we are demonstrably AT the destination — claim it by URL identity, record
    // the tolerance, and let the P5 assertions below own the business verdict.
    // Any other reason, or a real origin+path divergence, still fails closed —
    // now reporting the resolver's `rules` + ranked matches so the cause is
    // externally observable (result.metrics is stripped from the public/DB view).
    let dest: ScreenIdentityV01
    const liveRoute = routeTemplate(finalSync.value.url)
    const expectedRoute = routeTemplate(ir.expectedFinalScreen.url)
    if (finalRes.status === 'resolved' && finalRes.screen) {
      dest = finalRes.screen
      if (dest.screenId !== ir.expectedFinalScreen.screenId) {
        // Sibling of the B-1/B-4 drift tolerance, applied to the OTHER
        // destination-gate branch. Here the classifier confidently elected a
        // screen that is NOT the expected one. When the live page sits on the
        // SAME parameterized route as the expected destination — a flow that
        // CREATES a record lands on `/client/<newId>` while the learned episode
        // recorded `/client/<oldId>` — the identity differs only because each
        // created record renders a distinct detail signature (the company name
        // in the header), NOT because we are on a wrong page. Accept it by
        // route identity and let the P5 assertion below own the verdict; with no
        // assertion to lean on, or a genuinely off-route screen, it still fails
        // closed — so "transport completion + a destination match alone are
        // never green" is preserved.
        const sameRoute =
          (request.assertions?.length ?? 0) > 0 &&
          liveRoute !== null &&
          liveRoute === expectedRoute
        if (sameRoute) {
          dest = { ...ir.expectedFinalScreen, url: finalSync.value.url }
        } else {
          const rules = (finalRes.telemetry?.rules ?? []).join('+') || 'none'
          const top =
            (finalRes.matches ?? [])
              .slice(0, 3)
              .map((m) => `${m.screenId}:t${m.tier}:${m.confidence.toFixed(3)}`)
              .join(', ') || 'no-matches'
          return buildTerminal({
            request,
            ir,
            stepsRun,
            status: statusForReason('destination_mismatch'),
            failureReason: 'destination_mismatch',
            destination: dest,
            destinationScore: dest.confidence,
            path,
            recovery,
            bindingIdentity,
            startedAt,
            nowMs: runtime.now(),
            note: note(
              secrets,
              `final screen (${dest.screenId}) does not match the expected destination (${ir.expectedFinalScreen.screenId}); liveRoute=${liveRoute ?? 'n/a'} expRoute=${expectedRoute ?? 'n/a'}; rules=${rules}; top=[${top}]`
            ),
          })
        }
      }
    } else {
      const livePath = originPath(finalSync.value.url)
      const reason = finalRes.reason ?? 'screen_unknown'
      // Tolerate `screen_ambiguous` (twins on the SAME route) ONLY when a P5
      // assertion exists to independently confirm the business outcome — the
      // assertion gate runs next and forbids a false green. Route comparison is
      // parameter-insensitive (`routeTemplate`): a flow that CREATES a record
      // lands on a fresh id (`/client/13846`) and can never equal the learned
      // destination's exact path (`/client/13845`), so the id leaf is generalized.
      // With no assertions to lean on, transport + an unresolvable final screen
      // stays fail-closed, so "transport completion alone never becomes success"
      // is preserved.
      const sameDestination =
        reason === 'screen_ambiguous' &&
        (request.assertions?.length ?? 0) > 0 &&
        liveRoute !== null &&
        liveRoute === expectedRoute
      if (sameDestination) {
        // Destination confirmed by route identity; keep the expected
        // screen's identity fields, but report the URL actually observed.
        dest = { ...ir.expectedFinalScreen, url: finalSync.value.url }
      } else {
        const rules = (finalRes.telemetry?.rules ?? []).join('+') || 'none'
        const top =
          (finalRes.matches ?? [])
            .slice(0, 3)
            .map(
              (m) =>
                `${m.screenId}:t${m.tier}:${m.confidence.toFixed(3)}${
                  m.fingerprintExact ? '#fp' : ''
                }${m.originConflict ? '!origin' : ''}${m.authConflict ? '!auth' : ''}`
            )
            .join(', ') || 'no-matches'
        return buildTerminal({
          request,
          ir,
          stepsRun,
          status: statusForReason(reason),
          failureReason: reason,
          path,
          recovery,
          bindingIdentity,
          startedAt,
          nowMs: runtime.now(),
          note: note(
            secrets,
            `final screen could not be classified (${reason}; rules=${rules}; top=[${top}]; livePath=${livePath ?? 'n/a'} expPath=${originPath(ir.expectedFinalScreen.url) ?? 'n/a'}; liveRoute=${liveRoute ?? 'n/a'} expRoute=${routeTemplate(ir.expectedFinalScreen.url) ?? 'n/a'}) — no green claimed`
          ),
        })
      }
    }

    // ---- assertions gate (P5): business success additionally requires every
    // request assertion to evaluate to `passed` against the live final screen.
    // Unsupported shapes fail closed to `contract_conflict`; a failed assertion
    // is `step_failed`. Transport completion + destination match alone are
    // never a green. Reads only — the final verdict is never forced. ----
    if (request.assertions && request.assertions.length > 0) {
      const assertionOutcome = await evaluateFinalAssertions(
        ctx,
        Math.max(0, deadline - runtime.now())
      )
      if (!assertionOutcome.passed) {
        return postconditionTerminal(
          ctx,
          ir,
          assertionOutcome,
          stepsRun,
          path,
          recovery,
          bindingIdentity,
          startedAt,
          request,
          dest,
          dest.confidence,
          'final'
        )
      }
    }

    return buildTerminal({
      request,
      ir,
      stepsRun,
      status: 'success',
      destination: dest,
      destinationScore: dest.confidence,
      path,
      recovery,
      bindingIdentity,
      startedAt,
      nowMs: runtime.now(),
      verification: {
        status: 'passed',
        assertionsPassed: request.assertions?.length ?? 0,
        assertionsTotal: request.assertions?.length ?? 0,
      },
    })
  } catch (err) {
    const detail =
      err instanceof Error ? err.message : String(err)
    return buildTerminal({
      request,
      ir,
      stepsRun: 0,
      status: 'error',
      failureReason: 'error',
      path,
      recovery,
      startedAt,
      nowMs: runtime.now(),
      note: note(
        secrets,
        `executor threw (${detail.slice(0, 300) || 'unknown error'}) — terminal error; nothing further was mutated`
      ),
    })
  }
}

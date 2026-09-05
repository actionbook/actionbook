/**
 * P5 — Deterministic postconditions & business verification (lib/postconditions.ts).
 *
 * A typed, exhaustive assertion evaluator. It maps the frozen request-level
 * `TaskAssertionV01` shapes and IR step-level `StepPostconditionV01` shapes onto a
 * richer internal operator vocabulary, then evaluates those operators against a
 * safe, non-secret evidence bundle (`PostconditionEvidenceV01`) that the
 * executor collects from the injected runtime (URL + resolver projections +
 * live AX refs + modal/tab/iframe state + optional write probes + optional safe
 * network evidence).
 *
 * Honesty rules (P5):
 * - PURE. No I/O, no wall clock (elapsed ms is injected), no randomness, no LLM.
 *   Natural-language `custom_expression` assertions are NEVER interpreted — they
 *   fail closed as `unsupported`.
 * - FAIL CLOSED. Anything not deterministically representable from the safe
 *   evidence surface — `custom_expression`, empty or hostile (non-safe-regex)
 *   scopes, oversized inputs, out-of-range thresholds, missing required
 *   evidence fields, unknown/malformed shapes — yields an `unsupported` (or
 *   `error`) verdict. The executor maps those to the frozen canonical
 *   `contract_conflict` failure reason; P5 does NOT widen the frozen
 *   `BrowserTaskFailureReasonV01` union.
 * - DELAYED-DISAPPEARANCE HONESTY. `hidden` while the element is still
 *   present and failed count comparisons are NOT immediate negatives: DOM
 *   removal after save/delete animations lands late. Only the bounded poll
 *   budget decides, and exhaustion is reported as a timeout — never as an
 *   immediate negative, never as a pass.
 * - `passed` is only ever produced when the evidence the operator needs is
 *   actually present (availability flags on the evidence bundle). A verdict is
 *   never green on absence of evidence.
 * - Never echoes raw DOM, passwords, tokens, cookies, field values, selectors,
 *   `request.data` values, or model arguments: verdict summaries carry only the
 *   operator kind, verdict, safe source, reason token and timing. The field
 *   write-probe expression is boolean-only (`{ok, code?}`) and the dispatched
 *   value is the same value the mutation already sent to the bound runtime.
 */

import type {
  BrowserTaskRequestV01,
  StepPostconditionV01,
  TaskAssertionV01,
} from '../types/browser-task'

// ---------------------------------------------------------------------------
// Safe evidence (the only thing the executor is allowed to evaluate).
// ---------------------------------------------------------------------------

/** A live accessibility-tree node as surfaced by the bound runtime. */
export interface RefEvidenceV01 {
  refId: string
  role: string
  name?: string
  /**
   * P5 rework round 2 (finding 2): disabled state ONLY when the bound runtime
   * positively surfaces it. An ABSENT flag is no evidence either way —
   * `element_state` enabled/disabled fail closed (unsupported) on it, because
   * the production ref collector surfaces role/name only, and treating
   * absence as enabled reported a disabled production button as enabled.
   */
  disabled?: boolean
}

/**
 * Non-secret evidence for ONE assertion evaluation. `urlAvailable` /
 * `refsAvailable` distinguish "the source is genuinely empty" from "the source
 * could not be collected" so a failed collection can never masquerade as a
 * passing negative assertion.
 */
export interface PostconditionEvidenceV01 {
  urlAvailable: boolean
  url: string
  refsAvailable: boolean
  refs: RefEvidenceV01[]
  title: string | null
  headings: string[]
  screen: {
    screenId: number
    routeFamily?: string | null
    stateVariant?: string | null
  } | null
  screenConfidence: number | null
  modal: { open: boolean; kind?: string | null } | null
  /**
   * P5 rework round 3, finding 2 — distinguishes "the runtime collected the
   * modal channel and saw no modal" (`modal: null`, closed is a REAL claim)
   * from "the runtime never collected a modal channel" (production syncScreen
   * returns `modal: undefined`). `false` makes `modal_state: closed` fail
   * closed instead of reading an uncollected channel as "modal closed".
   * Evidence objects without the flag (legacy shape) keep the old behaviour.
   */
  modalAvailable?: boolean
  tabPanel: { selected: string; tabs: string[] } | null
  iframe: { present: boolean; identity?: string | null } | null
  /** Safe page-level status/alert lines the runtime surfaced (may be empty). */
  statusText: string[]
  /** Safe durable row/entity keys the runtime surfaced (may be empty). The
   * `record_id` placeholder branch REQUIRES the claimed id to appear here —
   * URL shape alone is never record evidence (round 2, finding 4). */
  rowKeys: string[]
  /**
   * P5 rework round 3, finding 3 — scope-attributed row keys for `row_exists`.
   * `null` = the row channel carries keys WITHOUT scope attribution (a scoped
   * assertion must fail closed on it: a same-keyed row from another
   * table/entity would otherwise pass). `undefined` = the channel was not
   * collected at all. Each entry attributes one `data-row-key` to one scope
   * token (normalized role / name / synthetic-affordance scope, mirroring
   * `refMatchesScope`).
   */
  rowScopes?: { scope: string; key: string }[] | null
  /** Optional per-step write probes keyed by normalized scope. */
  fieldProbes: Record<string, boolean>
  /** Optional safe network evidence (only when the runtime already collected it). */
  networkEvidence: { ok: boolean } | null
}

// ---------------------------------------------------------------------------
// Operator vocabulary.
// ---------------------------------------------------------------------------

export type PostconditionOperatorV01 =
  | { kind: 'url_equals'; url: string }
  | { kind: 'url_includes'; fragment: string }
  /** Composite used for the frozen `url_pattern` — equals / glob / path / route. */
  | { kind: 'url_matches'; pattern: string }
  | { kind: 'route_matches'; routeFamily: string }
  | { kind: 'screen_id_equals'; screenId: number }
  | { kind: 'state_variant_equals'; stateVariant: string }
  | { kind: 'screen_score_gte'; minScore: number }
  | {
      kind: 'element_state'
      scope: string
      state: 'visible' | 'hidden' | 'enabled' | 'disabled'
    }
  | { kind: 'field_equals'; scope: string; probe: boolean }
  | { kind: 'text_present'; text: string }
  | { kind: 'text_contains'; text: string }
  | { kind: 'modal_state'; state: 'open' | 'closed'; modalKind?: string }
  | { kind: 'same_url'; url: string }
  | {
      kind: 'count'
      scope: string
      op: 'eq' | 'gt' | 'gte' | 'lt' | 'lte'
      n: number
    }
  | { kind: 'record_id'; pattern: string }
  | { kind: 'field_contains'; scope: string; text: string }
  | { kind: 'status_text'; text: string; match: 'exact' | 'contains' }
  | {
      kind: 'tab_frame_state'
      target: 'tab' | 'frame'
      name?: string
      state: 'present' | 'absent' | 'selected'
    }
  | { kind: 'row_exists'; scope: string; key: string }
  | { kind: 'network_ok' }
  /** Explicit fail-closed marker for shapes P5 refuses to interpret. */
  | { kind: 'unsupported'; reason: string }

// ---------------------------------------------------------------------------
// Verdicts.
// ---------------------------------------------------------------------------

export type AssertionVerdictKindV01 = 'passed' | 'failed' | 'unsupported' | 'error'

export interface AssertionVerdictV01 {
  kind: AssertionVerdictKindV01
  /** Operator (or frozen assertion kind) this verdict is about. */
  operator: string
  /** Which safe evidence source decided the verdict. */
  source:
    | 'url'
    | 'screen'
    | 'refs'
    | 'title'
    | 'headings'
    | 'modal'
    | 'tab'
    | 'iframe'
    | 'probe'
    | 'network'
    | 'status'
    | 'rows'
    | 'none'
  /** Optional safe reason token (never echoes user data / selectors). */
  reason?: string
  /**
   * True when a `failed` verdict is an immediate negative: more polling with
   * the same settled page state cannot change it. Distinguishes immediate
   * negative evidence from a polling timeout.
   */
  definitive: boolean
  /** True when this verdict was reached only by exhausting the assertion budget. */
  timedOut: boolean
  /** Elapsed ms for this evaluation (injected — deterministic in tests). */
  elapsedMs: number
}

/** One operator + its label, as handed to the evaluator. */
export interface AssertionCheckV01 {
  index: number
  label: string
  operator: PostconditionOperatorV01
}

/**
 * The injectable evaluator seam. The executor's default is `defaultP5Evaluator`
 * (this module); tests may inject a forced evaluator to prove the real one is
 * load-bearing (fake-green guard).
 */
export type P5AssertionEvaluatorV01 = (
  checks: readonly AssertionCheckV01[],
  evidence: PostconditionEvidenceV01,
  nowMs: number
) => readonly AssertionVerdictV01[]

export interface EvaluateOptionsV01 {
  elapsedMs?: number
}

// ---------------------------------------------------------------------------
// Bounds (deterministic — never derived from wall clock or global state).
// ---------------------------------------------------------------------------

export const POSTCONDITION_LIMITS = {
  scopeMaxLen: 200,
  textMaxLen: 500,
  patternMaxLen: 500,
  countNMax: 100_000,
  rowKeyMaxLen: 200,
} as const

// ---------------------------------------------------------------------------
// Helpers.
// ---------------------------------------------------------------------------

function verdict(
  kind: AssertionVerdictKindV01,
  operator: string,
  source: AssertionVerdictV01['source'],
  opts: EvaluateOptionsV01,
  extra: Partial<Omit<AssertionVerdictV01, 'kind' | 'operator' | 'source' | 'elapsedMs'>> = {}
): AssertionVerdictV01 {
  return {
    kind,
    operator,
    source,
    elapsedMs: opts.elapsedMs ?? 0,
    definitive: false,
    timedOut: false,
    ...extra,
  }
}

function failed(
  operator: string,
  source: AssertionVerdictV01['source'],
  opts: EvaluateOptionsV01,
  extra: Partial<Omit<AssertionVerdictV01, 'kind' | 'operator' | 'source' | 'elapsedMs'>> = {}
): AssertionVerdictV01 {
  return verdict('failed', operator, source, opts, extra)
}

function passed(
  operator: string,
  source: AssertionVerdictV01['source'],
  opts: EvaluateOptionsV01
): AssertionVerdictV01 {
  return verdict('passed', operator, source, opts)
}

function unsupported(
  operator: string,
  reason: string,
  opts: EvaluateOptionsV01
): AssertionVerdictV01 {
  return verdict('unsupported', operator, 'none', opts, { reason })
}

/** Parse a URL pathname deterministically; degrade to the raw url on failure. */
function safePathname(url: string): string {
  try {
    return new URL(url).pathname
  } catch {
    return url.split('?')[0].split('#')[0]
  }
}

function compareCount(actual: number, op: 'eq' | 'gt' | 'gte' | 'lt' | 'lte', n: number): boolean {
  switch (op) {
    case 'eq':
      return actual === n
    case 'gt':
      return actual > n
    case 'gte':
      return actual >= n
    case 'lt':
      return actual < n
    case 'lte':
      return actual <= n
  }
}

// ---------------------------------------------------------------------------
// Scope matching (safe regex only — hostile selectors fail closed).
// ---------------------------------------------------------------------------

/** A scope may only contain word chars, spaces, underscores, hyphens. */
const SCOPE_SAFE_RE = /^[A-Za-z0-9][A-Za-z0-9 _-]{0,199}$/

/** Synthetic affordance scopes → the concrete AX roles they cover (P4 roles). */
const SYNTHETIC_SCOPE_ROLES: Readonly<Record<string, ReadonlySet<string>>> = {
  text_field: new Set(['textbox', 'searchbox', 'combobox']),
  secret_field: new Set(['textbox']),
  search_query: new Set(['searchbox', 'textbox']),
  file_field: new Set(['button']),
  branch_action: new Set(['button', 'link', 'menuitem']),
  auth_entry: new Set(['button', 'link']),
  toggle_field: new Set(['checkbox', 'switch']),
}

/** Normalize a scope; return null for empty / oversized / hostile scopes. */
function normalizeScope(raw: string): string | null {
  if (typeof raw !== 'string') return null
  const s = raw.trim()
  if (s.length === 0 || s.length > POSTCONDITION_LIMITS.scopeMaxLen) return null
  if (!SCOPE_SAFE_RE.test(s)) return null
  return s.toLowerCase()
}

/**
 * Does a live ref match a normalized scope? A ref matches by exact role,
 * by name substring (safe — evidence name is a truncated AX name, never echoed),
 * or by a synthetic affordance scope expanding to the ref's role.
 */
function refMatchesScope(node: RefEvidenceV01, scopeLower: string): boolean {
  const role = (node.role ?? '').toLowerCase()
  const name = (node.name ?? '').toLowerCase()
  if (role === scopeLower) return true
  if (name !== '' && name.includes(scopeLower)) return true
  const expansion = SYNTHETIC_SCOPE_ROLES[scopeLower]
  if (expansion && expansion.has(role)) return true
  return false
}

// ---------------------------------------------------------------------------
// Deterministic glob matching for url_pattern (`*` prefix/suffix/contains only).
// ---------------------------------------------------------------------------

/**
 * Does the url/path end with `tail`, or continue with a `/` right after it?
 * Anchors glob suffixes to a path boundary so `*3` matches `/client/3` and
 * `/client/3/` but NEVER `/client/123`.
 */
function urlAnchored(url: string, pathname: string, tail: string): boolean {
  if (url.endsWith(tail) || pathname.endsWith(tail)) return true
  const p = url.endsWith(tail + '/') ? url : pathname.endsWith(tail + '/') ? pathname : ''
  return p !== ''
}

/**
 * Deterministic glob matching for url_pattern (`*` prefix/suffix/contains).
 *
 * Boundary rules (P5 rework, finding 3): `prefix*` requires the prefix to end
 * with `/` and `a*b` anchors `b` to a path boundary — the old unanchored
 * `includes` made pattern `/client/1` pass URL `/client/123`.
 */
function globMatch(
  pattern: string,
  url: string,
  pathname: string
): 'pass' | 'fail' | 'unsupported' {
  const parts = pattern.split('*')
  const [a, b] = parts
  if (parts.length === 3 && a === '' && parts[2] === '' && b) {
    return url.includes(b) ? 'pass' : 'fail' // *sub* → contains
  }
  if (parts.length > 2) return 'unsupported' // multi-`*` globs fail closed
  if (a === '' && b === '') return 'unsupported' // bare `*`
  if (a === '') return url.endsWith(b) ? 'pass' : 'fail' // *suffix
  if (b === '') {
    // prefix* — only a path-boundary prefix is a valid route match
    if (!a.endsWith('/')) return 'unsupported' // e.g. `client*` fails closed
    return url.startsWith(a) || pathname.startsWith(a) ? 'pass' : 'fail'
  }
  // a*b — `a` must occur and `b` must end the url/path or be followed by `/`
  // (path boundary); the old `includes(a) && includes(b)` had no boundary and
  // let `/client/1` globs match `/client/123`.
  const hasA = url.includes(a) || pathname.includes(a)
  return hasA && urlAnchored(url, pathname, b) ? 'pass' : 'fail'
}

// ---------------------------------------------------------------------------
// Per-operator evaluation.
// ---------------------------------------------------------------------------

function evaluateUrlEquals(
  op: { kind: 'url_equals'; url: string },
  evidence: PostconditionEvidenceV01,
  opts: EvaluateOptionsV01
): AssertionVerdictV01 {
  if (!evidence.urlAvailable) {
    return verdict('error', 'url_equals', 'url', opts, { reason: 'url evidence unavailable' })
  }
  return evidence.url === op.url
    ? passed('url_equals', 'url', opts)
    : failed('url_equals', 'url', opts, { definitive: false, reason: 'url mismatch' })
}

function evaluateUrlIncludes(
  op: { kind: 'url_includes'; fragment: string },
  evidence: PostconditionEvidenceV01,
  opts: EvaluateOptionsV01
): AssertionVerdictV01 {
  if (!evidence.urlAvailable) {
    return verdict('error', 'url_includes', 'url', opts, { reason: 'url evidence unavailable' })
  }
  if (typeof op.fragment !== 'string' || op.fragment.length === 0) {
    return unsupported('url_includes', 'empty fragment', opts)
  }
  return evidence.url.includes(op.fragment)
    ? passed('url_includes', 'url', opts)
    : failed('url_includes', 'url', opts, { definitive: false, reason: 'fragment absent' })
}

function evaluateUrlMatches(
  op: { kind: 'url_matches'; pattern: string },
  evidence: PostconditionEvidenceV01,
  opts: EvaluateOptionsV01
): AssertionVerdictV01 {
  if (!evidence.urlAvailable) {
    return verdict('error', 'url_matches', 'url', opts, { reason: 'url evidence unavailable' })
  }
  if (typeof op.pattern !== 'string') {
    return unsupported('url_matches', 'malformed pattern', opts)
  }
  const pattern = op.pattern.trim()
  if (pattern.length === 0) return unsupported('url_matches', 'empty pattern', opts)
  if (pattern.length > POSTCONDITION_LIMITS.patternMaxLen) {
    return unsupported('url_matches', 'oversized pattern', opts)
  }
  const url = evidence.url
  if (url === pattern) return passed('url_matches', 'url', opts)
  const pathname = safePathname(url)
  if (pattern.includes('*')) {
    const g = globMatch(pattern, url, pathname)
    if (g === 'unsupported') return unsupported('url_matches', 'unsupported glob shape', opts)
    return g === 'pass'
      ? passed('url_matches', 'url', opts)
      : failed('url_matches', 'url', opts, { definitive: false, reason: 'no url/path match' })
  }
  // Path-shaped patterns (starting with `/`, no query/fragment) require an
  // EXACT pathname match (P5 rework, finding 3): the old startsWith/includes
  // matching let pattern `/client/1` pass URL `/client/123`.
  if (pattern.startsWith('/') && !pattern.includes('?') && !pattern.includes('#')) {
    return pathname === pattern
      ? passed('url_matches', 'url', opts)
      : failed('url_matches', 'url', opts, { definitive: false, reason: 'no exact url/path match' })
  }
  // Route-family fallback stays (the pattern names a route family, e.g. `app`).
  if (evidence.screen?.routeFamily === pattern) return passed('url_matches', 'screen', opts)
  // Non-path literal fallback (P5 rework round 2, finding 3): BOUNDED
  // containment, never `url.includes(pattern)` — that let the reviewer's
  // probe `https://app.test/client/1` pass URL `https://app.test/client/123`.
  // The occurrence must start at the URL's beginning or right after a `/`,
  // and end at the URL's end or right before one of `/?#`.
  return urlContainsBounded(url, pattern)
    ? passed('url_matches', 'url', opts)
    : failed('url_matches', 'url', opts, { definitive: false, reason: 'no url/path/route match' })
}

/**
 * Bounded containment for non-path URL patterns: some occurrence of `pattern`
 * in `url` starts at the URL's beginning or right after a path separator and
 * ends at the URL's end or right before one of `/?#`. Rejects prefix bleed:
 * `https://app.test/client/1` never matches `https://app.test/client/123`.
 */
function urlContainsBounded(url: string, pattern: string): boolean {
  if (pattern.length === 0 || url.length < pattern.length) return false
  let from = 0
  while (true) {
    const idx = url.indexOf(pattern, from)
    if (idx < 0) return false
    const startOk = idx === 0 || url[idx - 1] === '/'
    const endIdx = idx + pattern.length
    const endOk = endIdx === url.length || '/?#'.includes(url[endIdx])
    if (startOk && endOk) return true
    from = idx + 1
  }
}

function evaluateRouteMatches(
  op: { kind: 'route_matches'; routeFamily: string },
  evidence: PostconditionEvidenceV01,
  opts: EvaluateOptionsV01
): AssertionVerdictV01 {
  const route = evidence.screen?.routeFamily ?? ''
  if (!route) return failed('route_matches', 'screen', opts, { definitive: false })
  return route === op.routeFamily
    ? passed('route_matches', 'screen', opts)
    : failed('route_matches', 'screen', opts, { definitive: true, reason: 'route family mismatch' })
}

function evaluateScreenIdEquals(
  op: { kind: 'screen_id_equals'; screenId: number },
  evidence: PostconditionEvidenceV01,
  opts: EvaluateOptionsV01
): AssertionVerdictV01 {
  if (!evidence.screen) {
    return failed('screen_id_equals', 'screen', opts, { definitive: false, reason: 'screen unresolved' })
  }
  return evidence.screen.screenId === op.screenId
    ? passed('screen_id_equals', 'screen', opts)
    : failed('screen_id_equals', 'screen', opts, {
        definitive: true,
        reason: 'screen id mismatch',
      })
}

function evaluateStateVariantEquals(
  op: { kind: 'state_variant_equals'; stateVariant: string },
  evidence: PostconditionEvidenceV01,
  opts: EvaluateOptionsV01
): AssertionVerdictV01 {
  if (!evidence.screen) {
    return failed('state_variant_equals', 'screen', opts, { definitive: false, reason: 'screen unresolved' })
  }
  const variant = evidence.screen.stateVariant ?? ''
  return variant === op.stateVariant
    ? passed('state_variant_equals', 'screen', opts)
    : failed('state_variant_equals', 'screen', opts, { definitive: true, reason: 'state variant mismatch' })
}

function evaluateScreenScoreGte(
  op: { kind: 'screen_score_gte'; minScore: number },
  evidence: PostconditionEvidenceV01,
  opts: EvaluateOptionsV01
): AssertionVerdictV01 {
  // Fail closed (P5 rework, finding 6): a minScore outside [0, 1] is a
  // malformed threshold — the old clamp made `-1` pass nearly every resolved
  // screen. Never silently repair a contract value.
  if (typeof op.minScore !== 'number' || !Number.isFinite(op.minScore) || op.minScore < 0 || op.minScore > 1) {
    return unsupported('screen_score_gte', 'minScore must be a finite number within [0, 1]', opts)
  }
  if (evidence.screenConfidence === null) {
    return failed('screen_score_gte', 'screen', opts, { definitive: false, reason: 'screen unresolved' })
  }
  return evidence.screenConfidence >= op.minScore
    ? passed('screen_score_gte', 'screen', opts)
    : failed('screen_score_gte', 'screen', opts, { definitive: false, reason: 'score below threshold' })
}

function evaluateElementState(
  op: { kind: 'element_state'; scope: string; state: 'visible' | 'hidden' | 'enabled' | 'disabled' },
  evidence: PostconditionEvidenceV01,
  opts: EvaluateOptionsV01
): AssertionVerdictV01 {
  if (!evidence.refsAvailable) {
    return verdict('error', 'element_state', 'refs', opts, { reason: 'live AX evidence unavailable' })
  }
  const scope = normalizeScope(op.scope)
  if (!scope) return unsupported('element_state', 'unrepresentable scope', opts)
  const matches = evidence.refs.filter((n) => refMatchesScope(n, scope))
  if (op.state === 'visible') {
    return matches.length > 0
      ? passed('element_state', 'refs', opts)
      : failed('element_state', 'refs', opts, { definitive: false, reason: 'no matching live ref' })
  }
  if (op.state === 'enabled') {
    // P5 rework round 2, finding 2: ENABLED requires POSITIVE evidence — an
    // explicit `disabled === false` on a matching ref. The production ref
    // collector (`snapshotRefs`) and the runner's snapshot mapping do not
    // surface the flag, so treating an absent flag as enabled reported a
    // disabled production button as enabled. No evidence → unsupported →
    // `contract_conflict` terminal: the state is never green by assumption.
    if (matches.length === 0) {
      return failed('element_state', 'refs', opts, { definitive: false, reason: 'no matching live ref' })
    }
    if (!matches.every((n) => n.disabled === true || n.disabled === false)) {
      return unsupported('element_state', 'enabled state not evidenced on the matching ref', opts)
    }
    return matches.some((n) => n.disabled === true)
      ? failed('element_state', 'refs', opts, { definitive: false, reason: 'matching ref is disabled' })
      : passed('element_state', 'refs', opts)
  }
  if (op.state === 'disabled') {
    // Symmetric fail-closed: disabled requires an explicit `disabled === true`
    // on EVERY matching ref; an absent flag is no evidence either way.
    if (matches.length === 0) {
      return failed('element_state', 'refs', opts, { definitive: false, reason: 'no matching live ref' })
    }
    if (!matches.every((n) => n.disabled === true || n.disabled === false)) {
      return unsupported('element_state', 'disabled state not evidenced on the matching ref', opts)
    }
    return matches.every((n) => n.disabled === true)
      ? passed('element_state', 'refs', opts)
      : failed('element_state', 'refs', opts, { definitive: false, reason: 'matching ref is not disabled' })
  }
  // hidden
  // P5 rework, finding 4: a still-present element is NOT an immediate negative
  // — DOM removal after save/delete animations can land late, so the bounded
  // polling budget must be consumed before the verdict is final (timeout).
  return matches.length === 0
    ? passed('element_state', 'refs', opts)
    : failed('element_state', 'refs', opts, { definitive: false, reason: 'matching live ref is present' })
}

function evaluateFieldEquals(
  op: { kind: 'field_equals'; scope: string; probe: boolean },
  evidence: PostconditionEvidenceV01,
  opts: EvaluateOptionsV01
): AssertionVerdictV01 {
  const scope = normalizeScope(op.scope)
  const key = scope ?? op.scope
  if (evidence.fieldProbes[key] === undefined) {
    return unsupported('field_equals', 'no write probe computed for this scope', opts)
  }
  return evidence.fieldProbes[key] === true
    ? passed('field_equals', 'probe', opts)
    : failed('field_equals', 'probe', opts, { definitive: true, reason: 'field holds a different value' })
}

function evaluateText(
  op: { kind: 'text_present' | 'text_contains'; text: string },
  evidence: PostconditionEvidenceV01,
  opts: EvaluateOptionsV01
): AssertionVerdictV01 {
  if (typeof op.text !== 'string') return verdict('error', op.kind, 'none', opts, { reason: 'malformed text' })
  const text = op.text.trim()
  if (text.length === 0) return unsupported(op.kind, 'empty text', opts)
  if (text.length > POSTCONDITION_LIMITS.textMaxLen) return unsupported(op.kind, 'oversized text', opts)
  const haystack = [(evidence.title ?? ''), ...evidence.headings].join('\n').toLowerCase()
  const ok = haystack.includes(text.toLowerCase())
  return ok
    ? passed(op.kind, 'title', opts)
    : failed(op.kind, 'title', opts, { definitive: false, reason: 'not present in title/headings' })
}

function evaluateModalState(
  op: { kind: 'modal_state'; state: 'open' | 'closed'; modalKind?: string },
  evidence: PostconditionEvidenceV01,
  opts: EvaluateOptionsV01
): AssertionVerdictV01 {
  if (op.state === 'open') {
    const open = evidence.modal?.open === true
    if (!open) {
      return failed('modal_state', 'modal', opts, { definitive: false, reason: 'no open modal' })
    }
    // P5 rework round 2, finding 5: a requested modal KIND is part of the
    // assertion — the reviewer's probe showed `modalKind: 'settings'` passing
    // an open `dialog`. A kind mismatch is a definitive fail.
    if (op.modalKind !== undefined && evidence.modal?.kind !== op.modalKind) {
      return failed('modal_state', 'modal', opts, { definitive: true, reason: 'modal kind mismatch' })
    }
    return passed('modal_state', 'modal', opts)
  }
  // P5 rework round 3, finding 2: an UNCOLLECTED modal channel must not read
  // as "modal closed". Production syncScreen does not surface a modal channel
  // (it yields `modal: undefined`), so `modalAvailable === false` fails closed
  // — the closed claim needs a collected channel that shows no open modal.
  // P5 rework round 4, finding 3: the flag must be EXPLICITLY `true`. The
  // reviewer's probe showed flagless evidence (`modalAvailable` absent) still
  // passing closed — the round-3 legacy "null-means-closed" escape is gone.
  // `collectEvidence` always emits the flag as a boolean, so only hand-rolled
  // evidence hits this branch, and it must fail closed, not pass.
  if (evidence.modalAvailable !== true) {
    return verdict('error', 'modal_state', 'modal', opts, {
      reason: 'modal evidence unavailable — the bound runtime does not collect a modal channel',
    })
  }
  const closed = evidence.modal == null || evidence.modal.open === false
  return closed
    ? passed('modal_state', 'modal', opts)
    : failed('modal_state', 'modal', opts, { definitive: true, reason: 'modal is open' })
}

function evaluateSameUrl(
  op: { kind: 'same_url'; url: string },
  evidence: PostconditionEvidenceV01,
  opts: EvaluateOptionsV01
): AssertionVerdictV01 {
  if (!evidence.urlAvailable) {
    return verdict('error', 'same_url', 'url', opts, { reason: 'url evidence unavailable' })
  }
  return evidence.url === op.url
    ? passed('same_url', 'url', opts)
    : failed('same_url', 'url', opts, { definitive: false, reason: 'url changed' })
}

function evaluateCount(
  op: { kind: 'count'; scope: string; op: 'eq' | 'gt' | 'gte' | 'lt' | 'lte'; n: number },
  evidence: PostconditionEvidenceV01,
  opts: EvaluateOptionsV01
): AssertionVerdictV01 {
  if (!evidence.refsAvailable) {
    return verdict('error', 'count', 'refs', opts, { reason: 'live AX evidence unavailable' })
  }
  const scope = normalizeScope(op.scope)
  if (!scope) return unsupported('count', 'unrepresentable scope', opts)
  if (!Number.isFinite(op.n) || op.n < 0 || op.n > POSTCONDITION_LIMITS.countNMax) {
    return unsupported('count', 'invalid or oversized n', opts)
  }
  const count = evidence.refs.filter((n) => refMatchesScope(n, scope)).length
  const ok = compareCount(count, op.op, op.n)
  // P5 rework, finding 4: a count beyond its bound is NOT an immediate
  // negative — list rows / toast overlays can still settle within the bounded
  // budget, so only exhaustion of that budget is final (timeout).
  return ok
    ? passed('count', 'refs', opts)
    : failed('count', 'refs', opts, { definitive: false, reason: `observed ${count}` })
}

function evaluateRecordId(
  op: { kind: 'record_id'; pattern: string },
  evidence: PostconditionEvidenceV01,
  opts: EvaluateOptionsV01
): AssertionVerdictV01 {
  if (!evidence.urlAvailable) {
    return verdict('error', 'record_id', 'url', opts, { reason: 'url evidence unavailable' })
  }
  if (typeof op.pattern !== 'string') return unsupported('record_id', 'malformed pattern', opts)
  const pattern = op.pattern.trim()
  if (pattern.length === 0) return unsupported('record_id', 'empty pattern', opts)
  if (pattern.length > POSTCONDITION_LIMITS.patternMaxLen) {
    return unsupported('record_id', 'oversized pattern', opts)
  }
  const pathname = safePathname(evidence.url)
  // `/client/<id>` → the pattern is a route prefix with a durable id segment.
  const placeholders = [...pattern.matchAll(/<[^/>][^>]*>/g)]
  if (placeholders.length > 0) {
    // Exactly ONE placeholder, TERMINAL: nothing but the id may follow.
    if (placeholders.length !== 1 || !pattern.endsWith(placeholders[0][0])) {
      return unsupported('record_id', 'record pattern must have exactly one terminal placeholder', opts)
    }
    const prefix = pattern.slice(0, pattern.length - placeholders[0][0].length)
    if (!prefix.startsWith('/') || !prefix.endsWith('/')) {
      return unsupported('record_id', 'record prefix must start and end with a path separator', opts)
    }
    if (!pathname.startsWith(prefix)) {
      return failed('record_id', 'url', opts, { definitive: false, reason: 'durable record route absent' })
    }
    const residual = pathname.slice(prefix.length)
    // Durable evidence requires EXACTLY ONE id-shaped segment: `/client/new`
    // is a create page and `/client/foo/bar` is a nested route — neither is a
    // durable record id (P5 rework, finding 1).
    const reserved = new Set(['new', 'create', 'edit', 'add'])
    const idShape = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/
    const durable =
      residual !== '' &&
      !reserved.has(residual.toLowerCase()) &&
      idShape.test(residual)
    if (!durable) {
      return failed('record_id', 'url', opts, { definitive: false, reason: 'durable record route absent' })
    }
    // P5 rework round 2, finding 4: shape + denylist is NOT proof — any
    // non-reserved slug would pass, including static routes like
    // `/client/search` or `/client/settings`. The id segment must be
    // POSITIVELY evidenced: the bound runtime surfaced it as a durable
    // row/entity key. URL shape alone never greens a record route.
    if (!(evidence.rowKeys ?? []).includes(residual)) {
      return failed('record_id', 'rows', opts, { definitive: false, reason: 'record id not evidenced' })
    }
    return passed('record_id', 'rows', opts)
  }
  // Literal record route: exact path, or the prefix of a nested sub-route.
  return pathname === pattern || pathname.startsWith(pattern + '/')
    ? passed('record_id', 'url', opts)
    : failed('record_id', 'url', opts, { definitive: false, reason: 'record route absent' })
}

function evaluateFieldContains(
  op: { kind: 'field_contains'; scope: string; text: string },
  evidence: PostconditionEvidenceV01,
  opts: EvaluateOptionsV01
): AssertionVerdictV01 {
  if (typeof op.text !== 'string') return verdict('error', 'field_contains', 'none', opts, { reason: 'malformed text' })
  const text = op.text.trim()
  if (text.length === 0) return unsupported('field_contains', 'empty text', opts)
  if (text.length > POSTCONDITION_LIMITS.textMaxLen) return unsupported('field_contains', 'oversized text', opts)
  const scope = normalizeScope(op.scope)
  const key = scope ?? op.scope
  if (evidence.fieldProbes[key] === undefined) {
    return unsupported('field_contains', 'no write probe computed for this scope', opts)
  }
  return evidence.fieldProbes[key] === true
    ? passed('field_contains', 'probe', opts)
    : failed('field_contains', 'probe', opts, { definitive: true, reason: 'field does not contain the expected value' })
}

function evaluateStatusText(
  op: { kind: 'status_text'; text: string; match: 'exact' | 'contains' },
  evidence: PostconditionEvidenceV01,
  opts: EvaluateOptionsV01
): AssertionVerdictV01 {
  if (typeof op.text !== 'string') return verdict('error', 'status_text', 'none', opts, { reason: 'malformed text' })
  const text = op.text.trim()
  if (text.length === 0) return unsupported('status_text', 'empty text', opts)
  if (text.length > POSTCONDITION_LIMITS.textMaxLen) return unsupported('status_text', 'oversized text', opts)
  const lines = (evidence.statusText ?? []).map((l) => l.trim().toLowerCase())
  if (lines.length === 0) {
    // P5 rework round 2, finding 5: the bound runtime surfaces `statusText`
    // as `[]` today, so an empty channel means "not collected", never "text
    // not found". Fail CLOSED as unsupported → `contract_conflict`; a
    // negative verdict on an uncollected channel would be a false negative.
    return unsupported('status_text', 'no status channel collected by the bound runtime', opts)
  }
  const needle = text.toLowerCase()
  const ok =
    op.match === 'exact'
      ? lines.includes(needle)
      : lines.some((l) => l.includes(needle))
  return ok
    ? passed('status_text', 'status', opts)
    : failed('status_text', 'status', opts, { definitive: false, reason: 'status text not found' })
}

function evaluateTabFrameState(
  op: { kind: 'tab_frame_state'; target: 'tab' | 'frame'; name?: string; state: 'present' | 'absent' | 'selected' },
  evidence: PostconditionEvidenceV01,
  opts: EvaluateOptionsV01
): AssertionVerdictV01 {
  if (op.target === 'tab') {
    const panel = evidence.tabPanel
    if (!panel) {
      // P5 rework round 2, finding 5: the production runner does not collect
      // tab panels, so absent evidence is "not collected", never "absent".
      return unsupported('tab_frame_state', 'no tab evidence collected by the bound runtime', opts)
    }
    const tabs = panel.tabs ?? []
    if (op.state === 'present') {
      const ok = op.name ? tabs.includes(op.name) : tabs.length > 0
      return ok
        ? passed('tab_frame_state', 'tab', opts)
        : failed('tab_frame_state', 'tab', opts, { definitive: false, reason: 'tab not present' })
    }
    if (op.state === 'absent') {
      const ok = op.name ? !tabs.includes(op.name) : tabs.length === 0
      return ok
        ? passed('tab_frame_state', 'tab', opts)
        : failed('tab_frame_state', 'tab', opts, { definitive: false, reason: 'tab is present' })
    }
    // selected
    if (!op.name) return unsupported('tab_frame_state', 'selected tab requires a name', opts)
    return panel.selected === op.name
      ? passed('tab_frame_state', 'tab', opts)
      : failed('tab_frame_state', 'tab', opts, { definitive: false, reason: 'tab not selected' })
  }
  // frame
  const frame = evidence.iframe
  if (!frame) {
    // P5 rework round 2, finding 5: same channel semantics as tab evidence.
    return unsupported('tab_frame_state', 'no frame evidence collected by the bound runtime', opts)
  }
  if (op.state === 'present') {
    return frame.present
      ? passed('tab_frame_state', 'iframe', opts)
      : failed('tab_frame_state', 'iframe', opts, { definitive: false, reason: 'frame not present' })
  }
  if (op.state === 'absent') {
    return !frame.present
      ? passed('tab_frame_state', 'iframe', opts)
      : failed('tab_frame_state', 'iframe', opts, { definitive: false, reason: 'frame is present' })
  }
  return unsupported('tab_frame_state', 'frame selection is not deterministically observable', opts)
}

function evaluateRowExists(
  op: { kind: 'row_exists'; scope: string; key: string },
  evidence: PostconditionEvidenceV01,
  opts: EvaluateOptionsV01
): AssertionVerdictV01 {
  if (typeof op.key !== 'string') return verdict('error', 'row_exists', 'none', opts, { reason: 'malformed key' })
  const key = op.key.trim()
  if (key.length === 0) return unsupported('row_exists', 'empty key', opts)
  if (key.length > POSTCONDITION_LIMITS.rowKeyMaxLen) return unsupported('row_exists', 'oversized key', opts)
  const rows = evidence.rowKeys ?? []
  if (rows.length === 0) {
    // P5 rework round 2, finding 5: symmetric to status_text — the bound
    // runtime surfaces `rowKeys` as `[]` today, so an empty channel is
    // "not collected", not "row absent". Unsupported → `contract_conflict`.
    return unsupported('row_exists', 'no row/entity channel collected by the bound runtime', opts)
  }
  // P5 rework round 3, finding 3: the assertion's scope is LOAD-BEARING. A
  // page-wide `rows.includes(key)` would pass on a same-keyed row from a
  // different table/entity — the key must appear attributed to THIS scope.
  // Fail closed whenever the evidence cannot answer a scoped question:
  //   - `rowScopes` absent/`null` → the channel carries unscoped keys only;
  //   - the scope does not normalize → it cannot be matched deterministically.
  const scope = normalizeScope(op.scope)
  if (scope == null) {
    return unsupported('row_exists', 'scope not representable', opts)
  }
  const scopedRows = evidence.rowScopes
  if (scopedRows == null) {
    return unsupported('row_exists', 'row channel carries no scope attribution', opts)
  }
  return scopedRows.some((r) => r.scope === scope && r.key === key)
    ? passed('row_exists', 'rows', opts)
    : failed('row_exists', 'rows', opts, { definitive: false, reason: 'row key not found for scope' })
}

function evaluateNetworkOk(
  _op: { kind: 'network_ok' },
  evidence: PostconditionEvidenceV01,
  opts: EvaluateOptionsV01
): AssertionVerdictV01 {
  if (!evidence.networkEvidence) {
    return unsupported('network_ok', 'no safe network evidence collected', opts)
  }
  return evidence.networkEvidence.ok
    ? passed('network_ok', 'network', opts)
    : failed('network_ok', 'network', opts, { definitive: false, reason: 'network evidence not ok' })
}

// ---------------------------------------------------------------------------
// Exhaustive operator dispatcher.
// ---------------------------------------------------------------------------

export function evaluateOperator(
  op: PostconditionOperatorV01,
  evidence: PostconditionEvidenceV01,
  opts: EvaluateOptionsV01 = {},
  operatorLabel?: string
): AssertionVerdictV01 {
  const label = operatorLabel ?? op.kind
  switch (op.kind) {
    case 'url_equals':
      return evaluateUrlEquals(op, evidence, opts)
    case 'url_includes':
      return evaluateUrlIncludes(op, evidence, opts)
    case 'url_matches':
      return evaluateUrlMatches(op, evidence, opts)
    case 'route_matches':
      return evaluateRouteMatches(op, evidence, opts)
    case 'screen_id_equals':
      return evaluateScreenIdEquals(op, evidence, opts)
    case 'state_variant_equals':
      return evaluateStateVariantEquals(op, evidence, opts)
    case 'screen_score_gte':
      return evaluateScreenScoreGte(op, evidence, opts)
    case 'element_state':
      return evaluateElementState(op, evidence, opts)
    case 'field_equals':
      return evaluateFieldEquals(op, evidence, opts)
    case 'text_present':
    case 'text_contains':
      return evaluateText(op, evidence, opts)
    case 'modal_state':
      return evaluateModalState(op, evidence, opts)
    case 'same_url':
      return evaluateSameUrl(op, evidence, opts)
    case 'count':
      return evaluateCount(op, evidence, opts)
    case 'record_id':
      return evaluateRecordId(op, evidence, opts)
    case 'field_contains':
      return evaluateFieldContains(op, evidence, opts)
    case 'status_text':
      return evaluateStatusText(op, evidence, opts)
    case 'tab_frame_state':
      return evaluateTabFrameState(op, evidence, opts)
    case 'row_exists':
      return evaluateRowExists(op, evidence, opts)
    case 'network_ok':
      return evaluateNetworkOk(op, evidence, opts)
    case 'unsupported':
      return verdict('unsupported', label, 'none', opts, { reason: op.reason })
    default: {
      // Exhaustiveness guard: a newly added operator kind must be handled here.
      const never: never = op
      void never
      return verdict('error', label, 'none', opts, { reason: 'unhandled operator kind' })
    }
  }
}

// ---------------------------------------------------------------------------
// Mappers from the frozen contracts (fail closed on anything unrepresentable).
// ---------------------------------------------------------------------------

/** Map a frozen request `TaskAssertionV01` to an internal operator. */
export function operatorFromTaskAssertion(a: TaskAssertionV01): PostconditionOperatorV01 {
  if (!a || typeof a !== 'object') return { kind: 'unsupported', reason: 'malformed assertion' }
  switch (a.kind) {
    case 'text_present': {
      if (typeof a.text !== 'string') return { kind: 'unsupported', reason: 'malformed text' }
      return { kind: 'text_present', text: a.text }
    }
    case 'url_pattern': {
      if (typeof a.pattern !== 'string') return { kind: 'unsupported', reason: 'malformed pattern' }
      // A `<...>` placeholder turns a frozen url_pattern into a DURABLE RECORD
      // ROUTE assertion (`/client/<id>`): the executor must positively
      // evidence the id segment, not merely match url shape (P5 rework round
      // 2, finding 6). Patterns without a placeholder stay literal
      // url_matches — the frozen union is untouched, only its interpretation
      // is route-aware.
      if (a.pattern.includes('<')) {
        return { kind: 'record_id', pattern: a.pattern }
      }
      return { kind: 'url_matches', pattern: a.pattern }
    }
    case 'element_state': {
      if (typeof a.scope !== 'string' || !['visible', 'enabled', 'hidden'].includes(a.state)) {
        return { kind: 'unsupported', reason: 'unrepresentable element assertion' }
      }
      return { kind: 'element_state', scope: a.scope, state: a.state }
    }
    case 'count': {
      if (typeof a.selectorScope !== 'string') return { kind: 'unsupported', reason: 'malformed scope' }
      return { kind: 'count', scope: a.selectorScope, op: a.op, n: a.n }
    }
    case 'custom_expression':
      return {
        kind: 'unsupported',
        reason: 'custom_expression requires natural-language interpretation, which P5 forbids (no LLM, deterministic operators only)',
      }
    default: {
      // Exhaustiveness guard — an unknown frozen kind fails closed.
      const never: never = a
      void never
      return { kind: 'unsupported', reason: 'unknown assertion kind' }
    }
  }
}

/**
 * Resolve a frozen IR `StepPostconditionV01` (including `assertion{ref}`, which
 * references a request assertion by its non-negative integer index as a string)
 * to an operator + a stable label. Anything unresolvable fails closed.
 */
export function resolveStepPostcondition(
  pc: StepPostconditionV01,
  request: BrowserTaskRequestV01
): { operator: PostconditionOperatorV01; label: string } {
  if (!pc || typeof pc !== 'object') {
    return { operator: { kind: 'unsupported', reason: 'malformed postcondition' }, label: 'postcondition' }
  }
  switch (pc.kind) {
    case 'destination_screen_score': {
      // No clamp (P5 rework, finding 6): an out-of-range or non-finite minScore
      // must reach `evaluateScreenScoreGte` verbatim and fail closed there.
      const minScore = typeof pc.minScore === 'number' ? pc.minScore : NaN
      return { operator: { kind: 'screen_score_gte', minScore }, label: 'destination_screen_score' }
    }
    case 'url_pattern': {
      if (typeof pc.pattern !== 'string') {
        return { operator: { kind: 'unsupported', reason: 'malformed pattern' }, label: 'url_pattern' }
      }
      // Placeholder-bearing patterns are durable record-route assertions —
      // same route-aware interpretation as the request-level mapper (P5
      // rework round 2, finding 6).
      if (pc.pattern.includes('<')) {
        return { operator: { kind: 'record_id', pattern: pc.pattern }, label: 'url_pattern' }
      }
      return { operator: { kind: 'url_matches', pattern: pc.pattern }, label: 'url_pattern' }
    }
    case 'element_state': {
      return { operator: { kind: 'element_state', scope: pc.scope, state: pc.state }, label: 'element_state' }
    }
    case 'text_present': {
      if (typeof pc.text !== 'string') {
        return { operator: { kind: 'unsupported', reason: 'malformed text' }, label: 'text_present' }
      }
      return { operator: { kind: 'text_present', text: pc.text }, label: 'text_present' }
    }
    case 'assertion': {
      // Only a canonical decimal integer string is a valid index-as-string.
      // `Number('') === 0` and `Number('1.0') === 1` — empty/non-digit/wrapped
      // forms must fail closed, not silently alias an index.
      const raw = typeof pc.ref === 'string' ? pc.ref : ''
      if (!/^\d+$/.test(raw) || raw.length > 9) {
        return {
          operator: { kind: 'unsupported', reason: 'assertion ref must be a canonical non-negative integer index string' },
          label: 'assertion',
        }
      }
      const ref = Number(raw)
      if (!Number.isSafeInteger(ref) || ref < 0) {
        return {
          operator: { kind: 'unsupported', reason: 'assertion ref is out of representable range' },
          label: 'assertion',
        }
      }
      const assertions = request.assertions ?? []
      if (ref >= assertions.length) {
        return {
          operator: { kind: 'unsupported', reason: `assertion ref ${ref} is out of range (${assertions.length} assertions)` },
          label: 'assertion',
        }
      }
      return {
        operator: operatorFromTaskAssertion(assertions[ref]),
        label: `assertion[${ref}]`,
      }
    }
    default: {
      const never: never = pc
      void never
      return { operator: { kind: 'unsupported', reason: 'unknown postcondition kind' }, label: 'postcondition' }
    }
  }
}

/** Evaluate one frozen request assertion against evidence. */
export function evaluateTaskAssertion(
  a: TaskAssertionV01,
  evidence: PostconditionEvidenceV01,
  opts: EvaluateOptionsV01 = {}
): AssertionVerdictV01 {
  const label = (a && typeof a === 'object' && (a as { kind?: string }).kind) || 'assertion'
  return evaluateOperator(operatorFromTaskAssertion(a), evidence, opts, label)
}

/** Evaluate one frozen IR step postcondition against evidence. */
export function evaluateStepPostcondition(
  pc: StepPostconditionV01,
  request: BrowserTaskRequestV01,
  evidence: PostconditionEvidenceV01,
  opts: EvaluateOptionsV01 = {}
): AssertionVerdictV01 {
  const { operator, label } = resolveStepPostcondition(pc, request)
  return evaluateOperator(operator, evidence, opts, label)
}

// ---------------------------------------------------------------------------
// Aggregates.
// ---------------------------------------------------------------------------

/** The executor's default (real, deterministic) evaluator. */
export const defaultP5Evaluator: P5AssertionEvaluatorV01 = (checks, evidence, nowMs) =>
  checks.map((c) => evaluateOperator(c.operator, evidence, { elapsedMs: nowMs }, c.label))

/**
 * Compact, redacted evidence summary — operator kind, verdict, safe source,
 * safe reason token, timing only. Never echoes values, selectors, or DOM.
 */
export function summarizeAssertions(verdicts: readonly AssertionVerdictV01[]): string {
  if (verdicts.length === 0) return 'no assertions'
  return verdicts
    .map((v) => {
      const timing = v.timedOut ? ' (timeout)' : v.definitive ? ' (immediate-negative)' : ''
      const tail =
        v.kind === 'passed'
          ? ''
          : ` via ${v.source}${v.reason ? `: ${v.reason}` : ''}${timing}`
      return `${v.kind}:${v.operator}${tail}`
    })
    .join('; ')
}

// ---------------------------------------------------------------------------
// Field write-probe (boolean-only; never returns the field value).
// ---------------------------------------------------------------------------

export type FieldProbeOutcomeV01 = 'ok' | 'mismatch' | 'unverifiable'

/**
 * Build the safe browser expression that checks an element's value against the
 * expected bound value. The result is boolean-only: `{ ok:true }` on match,
 * `{ ok:false, code:'VALUE_MISMATCH' }` on mismatch, `{ ok:false, code:'NO_FIELD' }`
 * when the element is absent. It never echoes the value back.
 *
 * Modes: `equals` (default) compares the full value; `contains` checks that the
 * expected text occurs inside the value (P5 rework, finding 5).
 */
export function buildFieldCheckExpression(
  locator: string,
  expectedValue: string,
  opts: { match?: 'equals' | 'contains' } = {}
): string {
  const loc = JSON.stringify(locator)
  const exp = JSON.stringify(expectedValue)
  const cmp =
    opts.match === 'contains'
      ? `if(!String(el.value??'').includes(${exp})){return{ok:false,code:'VALUE_MISMATCH'}}`
      : `if(el.value!==${exp}){return{ok:false,code:'VALUE_MISMATCH'}}`
  return `(()=>{const el=document.querySelector(${loc});if(!el){return{ok:false,code:'NO_FIELD'}};${cmp};return{ok:true};})()`
}

/**
 * Interpret a dispatch envelope for a field write-probe. Only a definite
 * page-level `{ ok:false }` is a `mismatch`; a transport error or an envelope
 * without a boolean page result is `unverifiable`. The EXECUTOR decides what
 * an unverifiable probe means: when verification is required it fails the step
 * closed (finding 2) — it is never silently green.
 */
export function interpretFieldProbe(raw: unknown): FieldProbeOutcomeV01 {
  if (!raw || typeof raw !== 'object') return 'unverifiable'
  const envelope = raw as { ok?: boolean; result?: { value?: unknown } }
  if (envelope.ok !== true) return 'unverifiable'
  const page = envelope.result?.value as { ok?: boolean } | undefined
  if (page && page.ok === true) return 'ok'
  if (page && page.ok === false) return 'mismatch'
  return 'unverifiable'
}

// ---------------------------------------------------------------------------
// Row-key probe (P5 rework round 2, finding 6; round 3, finding 3). The
// durable-record evidence a `record_id` assertion demands: the runtime
// surfaces entity/row keys present in the bound document's STRUCTURE (the
// `data-row-key` attribute) as a bounded string list. Reads only — no DOM
// mutation, no input values, no field echoes; the page result is
// `{ok:true, rowKeys:[...]}` with every entry length-bounded in the
// interpreter. This is the positive proof channel that replaces url-shape
// guessing: a `/client/<id>` route greens ONLY when the residual id is one of
// the surfaced row keys.
//
// Round 3, finding 3: `row_exists` is SCOPED — a same-keyed row from a
// different table/entity must not pass. With non-empty `scopes` the
// expression ALSO attributes each keyed row to matching scope tokens (role
// exact / name contains / synthetic-affordance expansion, mirroring
// `refMatchesScope` below) and returns `rows:[{scope,key}]` with `scoped:true`.
// A scope token that cannot be represented page-side (it does not normalize)
// yields `scoped:false` — the evaluator then fails the scoped assertion
// closed instead of guessing. With no scopes the expression keeps the exact
// round-2 shape (`rowKeys` only) so unscoped `record_id` channels are
// unchanged.
// ---------------------------------------------------------------------------

/** Max row keys the structured page operation may return (collection-side bound). */
export const ROW_KEY_PROBE_LIMITS = { maxKeys: 128, maxKeyLen: 200 } as const

/**
 * Build the safe browser expression that harvests row/entity keys from the
 * document. Bounded by construction: a fixed attribute, an attribute-selector
 * scan (never a full DOM walk of arbitrary subtrees), and a hard cap on both
 * the key count and key length in the expression itself.
 */
export function buildRowKeyProbeExpression(): string {
  const maxKeys = ROW_KEY_PROBE_LIMITS.maxKeys
  const maxKeyLen = ROW_KEY_PROBE_LIMITS.maxKeyLen
  return `(()=>{const out=[];const els=document.querySelectorAll('[data-row-key]');for(let i=0;i<els.length&&out.length<${maxKeys};i++){const k=String(els[i].getAttribute('data-row-key')??'').slice(0,${maxKeyLen});if(k)out.push(k)}return{ok:true,rowKeys:out};})()`
}

/**
 * P5 rework round 3, finding 3 — the scope-aware variant. Every requested
 * scope must normalize (`normalizeScope`); otherwise the expression still runs
 * but reports `scoped:false` so the scoped assertion fails closed page-side.
 * Scope attribution mirrors `refMatchesScope`: a keyed row's role token (its
 * `data-row-role`, tag-derived role, or a `role`-attribute value), its name
 * tokens (`data-row-name` / `aria-label` / heading text), and any synthetic
 * affordance expansion that covers the row's role.
 *
 * P5 rework round 4, finding 4 — the two channels are independent: `rowKeys`
 * harvests EVERY keyed row (the durable `record_id` channel), while `rows`
 * carries only the scope-attributed `{scope,key}` pairs. The round-3 shape
 * derived `rowKeys` from the scope-matched rows, silently dropping durable
 * keys outside the requested scope (a record_id + scoped row_exists battery
 * lost the record key).
 */
export function buildRowKeyProbeExpressionWithScopes(scopes: readonly string[]): string {
  const maxKeys = ROW_KEY_PROBE_LIMITS.maxKeys
  const maxKeyLen = ROW_KEY_PROBE_LIMITS.maxKeyLen
  const normalized = scopes.map((s) => normalizeScope(s))
  const representable = normalized.every((n) => n !== null)
  const tokens = JSON.stringify(normalized.filter((n): n is string => n !== null))
  const synth = JSON.stringify(SYNTHETIC_SCOPE_ROLES, (_k, v) => (v instanceof Set ? [...v] : v))
  if (!representable) {
    // Unrepresentable scope: harvest the plain keys (the record_id channel may
    // still use them) but mark the channel unscoped — a scoped assertion must
    // fail closed on it.
    return `(()=>{const out=[];const els=document.querySelectorAll('[data-row-key]');for(let i=0;i<els.length&&out.length<${maxKeys};i++){const k=String(els[i].getAttribute('data-row-key')??'').slice(0,${maxKeyLen});if(k)out.push(k)}return{ok:true,rowKeys:out,rows:[],scoped:false};})()`
  }
  return `(()=>{const T=${tokens};const SYN=${synth};const seen=new Set();const out=[];const rows=[];const els=document.querySelectorAll('[data-row-key]');for(let i=0;i<els.length&&out.length<${maxKeys};i++){const el=els[i];const k=String(el.getAttribute('data-row-key')??'').slice(0,${maxKeyLen});if(!k){continue};out.push(k);const role=(el.getAttribute('data-row-role')||el.getAttribute('role')||(/^tr$/i.test(el.tagName)?'row':el.tagName)).toLowerCase();const name=(el.getAttribute('data-row-name')||el.getAttribute('aria-label')||'').toLowerCase();const tokens=[];for(const t of T){if(role===t){tokens.push(t);continue};if(name&&name.includes(t)){tokens.push(t);continue};const ex=SYN[t];if(ex&&ex.indexOf(role)>=0){tokens.push(t)}};for(const s of tokens){const sig=s+' '+k;if(seen.has(sig)){continue};seen.add(sig);rows.push({scope:s,key:k})}}return{ok:true,rowKeys:out,rows:rows,scoped:true};})()`
}

/**
 * Interpret a dispatch envelope for the row-key probe. Only a definite
 * page-level `{ ok:true, rowKeys:[...] }` yields keys (bounded again here,
 * defensively); every other envelope — transport failure, missing page
 * result, or a non-array `rowKeys` — is `null`, and the caller treats it as
 * an empty channel (the record assertion then fails closed).
 */
export function interpretRowKeyProbe(raw: unknown): string[] | null {
  if (!raw || typeof raw !== 'object') return null
  const envelope = raw as { ok?: boolean; result?: { value?: unknown } }
  if (envelope.ok !== true) return null
  const page = envelope.result?.value as { ok?: boolean; rowKeys?: unknown } | undefined
  if (!page || page.ok !== true || !Array.isArray(page.rowKeys)) return null
  const keys: string[] = []
  for (const k of page.rowKeys) {
    if (keys.length >= ROW_KEY_PROBE_LIMITS.maxKeys) break
    if (typeof k !== 'string') continue
    const trimmed = k.slice(0, ROW_KEY_PROBE_LIMITS.maxKeyLen)
    if (trimmed) keys.push(trimmed)
  }
  return keys
}

/**
 * Interpret a dispatch envelope for the SCOPED row-key probe (round 3,
 * finding 3). Returns the scoped rows, or `null` when the envelope is
 * malformed/transport-failed, the page reports `scoped:false` (a requested
 * scope was unrepresentable), or any row entry is malformed. The caller maps
 * `null` onto an unscoped channel, and the evaluator fails a scoped
 * `row_exists` closed on it.
 */
export function interpretRowKeyProbeWithScopes(
  raw: unknown
): { scope: string; key: string }[] | null {
  if (!raw || typeof raw !== 'object') return null
  const envelope = raw as { ok?: boolean; result?: { value?: unknown } }
  if (envelope.ok !== true) return null
  const page = envelope.result?.value as
    | { ok?: boolean; scoped?: unknown; rows?: unknown }
    | undefined
  if (!page || page.ok !== true || page.scoped !== true || !Array.isArray(page.rows)) return null
  const rows: { scope: string; key: string }[] = []
  for (const r of page.rows) {
    if (rows.length >= ROW_KEY_PROBE_LIMITS.maxKeys) break
    if (!r || typeof r !== 'object') continue
    const row = r as { scope?: unknown; key?: unknown }
    if (typeof row.scope !== 'string' || typeof row.key !== 'string') continue
    const key = row.key.slice(0, ROW_KEY_PROBE_LIMITS.maxKeyLen)
    if (!key) continue
    rows.push({ scope: row.scope, key })
  }
  return rows
}

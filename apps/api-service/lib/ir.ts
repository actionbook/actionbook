/**
 * P3 — Task Compiler IR + pure derivation helpers (lib/ir.ts).
 *
 * Pure, deterministic, synchronous. Zero I/O: no browser/extension/CDP/DB/
 * Redis/fetch/LLM/embedding/discovery-enqueue/background-job/timer reads and
 * no global-config reads. No `Date.now()` anywhere. All hashing is SHA-256
 * over canonical UTF-8 bytes (C2).
 *
 * The concrete compile pipeline lives in `lib/task-compiler.ts`. This file
 * carries the P3-owned IR surface:
 *   - the C2 scenario derivation (`deriveScenarioId` / `deriveScenarioVersion`)
 *   - the deterministic `irId` derivation over a canonical secret-free projection
 *   - a byte-identical port of the pure route-family helpers from
 *     `services/db/src/route-template.ts` (that module cannot be imported here:
 *     the `@actionbookdev/db` index loads env-coupled `./connection` at module
 *     load, which would break compiler purity — the port is the screen-signature
 *     redeclaration pattern)
 *   - the frozen `BrowserFlowIRV01` type re-exports (never duplicated here)
 *   - the P3-only internal derivation types, `COMPILER_LIMITS`, and the
 *     deterministic step/IR-shaping helpers the compiler composes.
 *
 * Frozen-contract rule: only `types/browser-task.ts` owns `BrowserFlowIRV01`.
 * Everything defined here is either a re-export of that file or a P3-private
 * derivation type.
 */
import { createHash } from 'node:crypto'

export { BROWSER_TASK_VERSION } from '../types/browser-task'

// Frozen v0.1 contract re-exports (type-only; no runtime cost, no I/O).
// The `import type` block below also binds the names locally for the P3
// derivation types; the `export type ... from` re-exports the contract surface.
export type {
  BrowserFlowIRV01,
  FlowIRStepV01,
  DataSlotBindingV01,
  ScreenIdentityV01,
  StepPreconditionV01,
  StepPostconditionV01,
  AuthModeV01,
  BrowserTaskRequestV01,
  BrowserTaskFailureReasonV01,
} from '../types/browser-task'

import type {
  AuthModeV01,
  BrowserFlowIRV01,
  BrowserTaskRequestV01,
  ScreenIdentityV01,
} from '../types/browser-task'

// ---------------------------------------------------------------------------
// Pure route-family helpers — byte-identical port of
// services/db/src/route-template.ts (fixed segment depth 3, no env reads).
// ---------------------------------------------------------------------------

const TRACKING_PARAMS = new Set([
  '_rsc',
  'utm_source',
  'utm_medium',
  'utm_campaign',
  'utm_term',
  'utm_content',
  'gclid',
  'fbclid',
  'ref',
  'ref_src',
])

const DETAIL_QUERY_KEYS = [
  'id',
  'slug',
  'sku',
  'item',
  'product',
  'post',
  'article',
  'doc',
  'entry',
]

const UTILITY_QUERY_KEYS = [
  'page',
  'sort',
  'filter',
  'tab',
  'view',
  'q',
  'query',
  'search',
  'lang',
  'locale',
]

const ROUTE_FAMILY_SEGMENT_DEPTH = 3

function isRouteLikeHash(hash: string): boolean {
  return /^#!?\//.test(hash.trim())
}

/** Canonical route key (identical to services/db/src/route-template.ts). */
export function normalizeRouteKey(url: string): string {
  try {
    const parsed = new URL(url)
    const normalizedHash = parsed.hash.trim()
    if (!isRouteLikeHash(normalizedHash)) {
      parsed.hash = ''
    }
    const keptParams = [...parsed.searchParams.entries()]
      .filter(([key]) => !TRACKING_PARAMS.has(key.toLowerCase()))
      .sort(([leftKey, leftValue], [rightKey, rightValue]) => {
        if (leftKey === rightKey) return leftValue.localeCompare(rightValue)
        return leftKey.localeCompare(rightKey)
      })

    parsed.search = ''
    for (const [key, value] of keptParams) {
      parsed.searchParams.append(key, value)
    }
    if (parsed.pathname.length > 1) {
      parsed.pathname = parsed.pathname.replace(/\/+$/, '')
    }
    return parsed.toString()
  } catch {
    return url.trim()
  }
}

function hasDetailSemanticHint(semanticHints?: string[]): boolean {
  return (semanticHints ?? []).some(
    (hint) =>
      hint === 'detail_branch' ||
      hint.includes('detail') ||
      hint.endsWith('_detail_link') ||
      hint.endsWith('_detail_entry')
  )
}

function looksLikeParameterizedLeaf(args: {
  segment: string
  index: number
  totalSegments: number
  semanticHints?: string[]
}): boolean {
  if (args.index === 0 || args.index !== args.totalSegments - 1) return false
  const normalized = args.segment.toLowerCase()
  if (/^\d+$/.test(normalized)) return true
  if (/^[a-f0-9]{8,}$/i.test(normalized)) return true
  if (/^[a-z0-9_-]*\d[a-z0-9_-]*$/i.test(normalized) && normalized.length >= 6) {
    return true
  }
  if (hasDetailSemanticHint(args.semanticHints)) return true
  if (normalized.length >= 12) return true
  return false
}

function hasDetailQueryShape(url: string): boolean {
  try {
    const parsed = new URL(url)
    const keys = [...parsed.searchParams.keys()].map((key) => key.toLowerCase())
    const hasDetailKey = keys.some((key) => DETAIL_QUERY_KEYS.includes(key))
    const hasUtilityKey = keys.some((key) => UTILITY_QUERY_KEYS.includes(key))
    return hasDetailKey && !hasUtilityKey
  } catch {
    return false
  }
}

function normalizeFamilySegment(
  segment: string,
  index: number,
  allSegments: string[],
  semanticHints?: string[]
): string {
  const normalized = segment.toLowerCase()
  if (/^\d+$/.test(normalized)) return ':n'
  if (/^[a-f0-9]{8,}$/i.test(normalized)) return ':id'
  if (/^[a-z0-9_-]*\d[a-z0-9_-]*$/i.test(normalized) && normalized.length >= 6) {
    return normalized.replace(/\d+/g, ':n')
  }
  if (
    looksLikeParameterizedLeaf({
      segment: normalized,
      index,
      totalSegments: allSegments.length,
      semanticHints,
    })
  ) {
    return ':detail'
  }
  return normalized
}

function routeHashFamilySuffix(args: {
  url: string
  semanticHints?: string[]
}): string | undefined {
  try {
    const parsed = new URL(args.url)
    const hash = parsed.hash.trim()
    if (!isRouteLikeHash(hash)) return undefined
    const hashRoute = hash.replace(/^#!/, '').replace(/^#/, '')
    const pathPart = hashRoute.split(/[?#]/, 1)[0] ?? ''
    const segments = pathPart
      .split('/')
      .filter(Boolean)
      .slice(0, ROUTE_FAMILY_SEGMENT_DEPTH)
      .map((segment, index, all) =>
        normalizeFamilySegment(segment, index, all, args.semanticHints)
      )
    return segments.length > 0 ? segments.join('/') : undefined
  } catch {
    return undefined
  }
}

/** Route family key (identical to services/db/src/route-template.ts). */
export function routeFamilyKey(args: { url: string; semanticHints?: string[] }): string {
  try {
    const parsed = new URL(args.url)
    const segments = parsed.pathname
      .split('/')
      .filter(Boolean)
      .slice(0, ROUTE_FAMILY_SEGMENT_DEPTH)
      .map((segment, index, all) =>
        normalizeFamilySegment(segment, index, all, args.semanticHints)
      )
    const baseFamily = segments.length > 0 ? segments.join('/') : '/'
    const hashFamily = routeHashFamilySuffix(args)
    if (hasDetailQueryShape(args.url)) {
      return `${baseFamily}:query-detail${hashFamily ? `#${hashFamily}` : ''}`
    }
    return hashFamily ? `${baseFamily}#${hashFamily}` : baseFamily
  } catch {
    return args.url.trim()
  }
}

// ---------------------------------------------------------------------------
// Graph-chain pure helper ports (graph-chain-planner.ts). The delegation
// requires porting transitionNode / requiredSlots / urlsLikelySame /
// safeRouteFamily / findLowestCostPath / edgeCost (minus Date.now terms) as
// pure projections. The `edgeCost` Date.now age-decay is deliberately dropped
// (it would break determinism); every other cost term is ported into the P3
// chain scorer in task-compiler.ts.
// ---------------------------------------------------------------------------

export function finiteNumber(value: number | null | undefined): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : 0
}

/** `{origin}{pathname}` with trailing slash stripped (graph-chain port). */
export function comparableUrl(value: string | null | undefined): string | undefined {
  const normalized = value?.trim()
  if (!normalized) return undefined
  try {
    const url = new URL(normalized)
    return `${url.origin}${url.pathname}`.replace(/\/$/, '')
  } catch {
    return normalized.replace(/\/$/, '')
  }
}

function isOriginOnlyUrl(value: string): boolean {
  try {
    const url = new URL(value)
    return value.replace(/\/$/, '') === url.origin
  } catch {
    return false
  }
}

/** Same-page heuristic (graph-chain port, Date-free). */
export function urlsLikelySame(left: string | null | undefined, right: string | null | undefined): boolean {
  const l = comparableUrl(left)
  const r = comparableUrl(right)
  if (!l || !r) return false
  if (l === r) return true
  if (isOriginOnlyUrl(l) || isOriginOnlyUrl(r)) return false
  return l.startsWith(`${r}/`) || r.startsWith(`${l}/`)
}

/** routeFamilyKey wrapped for a URL, undefined-safe (graph-chain port). */
export function safeRouteFamily(url: string | null | undefined): string | undefined {
  if (!url) return undefined
  try {
    return routeFamilyKey({ url })
  } catch {
    return undefined
  }
}

// ---------------------------------------------------------------------------
// SHA-256 primitives (C2).
// ---------------------------------------------------------------------------

/** Deterministic SHA-256 over canonical UTF-8 bytes. */
export function sha256Utf8(text: string): string {
  return createHash('sha256').update(text, 'utf8').digest('hex')
}

/**
 * C2 scenario derivation. Exact formula:
 *   scenarioId = sha256(route_family + '>' + ordered transition_ids joined by '>')
 * Normalized route family; the transition chain order is preserved; numeric
 * screen ids, requestId, secrets, timestamps, row order and replay timestamps
 * are excluded by construction.
 */
export function deriveScenarioId(routeFamily: string, orderedTransitionIds: string[]): string {
  const family = routeFamily.trim()
  return sha256Utf8(`${family}>${orderedTransitionIds.join('>')}`)
}

/** C2 scenario version (frozen). */
export const SCENARIO_VERSION = '0.1' as const

export function deriveScenarioVersion(): '0.1' {
  return SCENARIO_VERSION
}

/**
 * Structural C2 conformance check for a pre-derived scenario candidate. A
 * scenario is only accepted as a `scenario`-tier path when its declared
 * scenarioId matches the canonical derivation AND its version matches the
 * frozen SCENARIO_VERSION. Anything else (stale version, forged/incorrect id,
 * wrong family>ids) is a plain transition chain at best — it must never win
 * unconditionally.
 */
export function scenarioMatchesC2Derivation(scenario: CompilerScenarioV01): boolean {
  if (scenario.scenarioVersion !== SCENARIO_VERSION) return false
  return scenario.scenarioId === deriveScenarioId(scenario.routeFamily, scenario.transitionIds)
}

/**
 * Deterministic `irId` over a canonical secret-free projection. The compiler
 * builds the projection (an ordered, normalized array of strings), and this
 * hashes it. Same request + same screen + same chain ⇒ same bytes; only the
 * data *names* participate, never the data *values*.
 */
export function deriveIrId(canonicalProjection: readonly string[]): string {
  return sha256Utf8(canonicalProjection.join('|'))
}

// ---------------------------------------------------------------------------
// Canonicalization helpers (deterministic across Date/string/null variants).
// ---------------------------------------------------------------------------

/** Canonical timestamp string; Date → ISO, string → trimmed, else ''. */
export function canonicalTimestamp(value: Date | string | null | undefined): string {
  if (value instanceof Date) return value.toISOString()
  if (typeof value === 'string') return value.trim()
  return ''
}

/** Canonical string; null/undefined → ''. */
export function canonicalString(value: string | null | undefined): string {
  return typeof value === 'string' ? value.trim() : ''
}

/** Canonical number with a deterministic fallback. */
export function canonicalNumber(value: number | null | undefined, fallback: number): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback
}

/**
 * Closed allowlist of step-target roles that may appear verbatim in
 * structured logs (B-14 re-review). Template roles are free-form strings and
 * CAN carry page/user text (e.g. `private_customer_john_doe`), so a syntactic
 * check (lowercase/charset) is not sufficient — only these trusted tokens
 * pass. Membership:
 *   - synthetic form roles: the compiler/recorder contract vocabulary
 *     (`flow-executor.ts` SYNTHETIC_ROLE_ROLES keys);
 *   - compiler-owned fallbacks emitted by `templateTargetToIr`;
 *   - real AX roles: platform vocabulary (`flow-executor.ts`
 *     CLICKABLE_ROLES/FILLABLE_ROLES), never page content.
 * Anything else degrades to a constant placeholder. Kept local (mirroring
 * those sets) because `flow-executor`/`task-compiler` import this module —
 * importing them back would be a cycle.
 */
export const LOG_SAFE_TARGET_ROLES: ReadonlySet<string> = new Set([
  'text_field',
  'secret_field',
  'search_query',
  'file_field',
  'branch_action',
  'auth_entry',
  'toggle_field',
  'element',
  'element_locator',
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
  'spinbutton',
])

/** Closed allowlist of step-target scopes (the IR union, fixed). */
export const LOG_SAFE_TARGET_SCOPES: ReadonlySet<string> = new Set(['screen', 'element'])

/**
 * Secret-safe rendering of a single semantic role for inline diagnostic
 * strings (B-14 re-review). Template roles are free-form text; only
 * allowlisted tokens pass, everything else degrades to a constant
 * placeholder. Callers keep their own `?? 'element'` default so the common
 * unscoped case stays byte-identical.
 */
export function scrubSemanticRoleForLog(role: string | undefined): string {
  return typeof role === 'string' && LOG_SAFE_TARGET_ROLES.has(role) ? role : '<role>'
}

/**
 * Secret-safe rendering of a compiled step target for structured logs (B-14).
 *
 * A compiled `target.ref` is usually the canonical semantic ref (`role@index`)
 * — but `pattern`-scope and `navigate` steps carry the template's RAW
 * locator/URL verbatim, and `locatorCandidates` always holds stored selectors
 * (log those as a count, never the values). Raw selector text is
 * page-specific content that must never reach logs. A ref passes through only
 * when its role is allowlisted AND its index is numeric; everything else
 * degrades to scope/role shape tokens, never the value.
 */
export function scrubStepTargetForLog(
  target: { scope?: string; role?: string; ref?: string } | null | undefined
): string {
  const scope =
    typeof target?.scope === 'string' && target.scope
      ? LOG_SAFE_TARGET_SCOPES.has(target.scope)
        ? target.scope
        : 'unknown'
      : 'screen'
  const role =
    typeof target?.role === 'string' && LOG_SAFE_TARGET_ROLES.has(target.role) ? target.role : null
  const ref = typeof target?.ref === 'string' ? target.ref : null
  if (ref !== null) {
    // Linear match, no nested quantifiers. The role must be allowlisted AND
    // the index a bounded non-negative safe integer, reserialized
    // canonically — an unbounded digit run (or anything else) would make the
    // log field itself unbounded, so it degrades to the placeholder instead.
    const m = ref.match(/^([^@]+)@(\d+)$/)
    if (m !== null && LOG_SAFE_TARGET_ROLES.has(m[1])) {
      const digits = m[2]
      if (digits.length <= 16) {
        const index = Number(digits)
        if (Number.isSafeInteger(index) && index >= 0) return `${m[1]}@${index}`
      }
    }
  }
  const roleSuffix = role !== null ? `:${role}` : ''
  // No ref at all: unbound shape, nothing withheld. Any other string is a raw
  // locator/URL (or an overlong/non-safe index) whose value stays out of the log.
  if (ref === null) return `${scope}${roleSuffix}`
  return `${scope}${roleSuffix}:<locator>`
}

/**
 * Bounded lexical normalization for goal text: lowercase, collapse
 * non-alphanumeric runs to single spaces, collapse whitespace, trim.
 * Deterministic and locale-independent (no `toLowerCase` locale args).
 */
export function normalizeTokenText(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
}

/** Very common stopwords dropped from goal tokens. */
const GOAL_STOPWORDS = new Set([
  'a', 'an', 'the', 'and', 'or', 'to', 'of', 'for', 'with', 'on', 'in',
  'at', 'by', 'via', 'be', 'is', 'are', 'was', 'were', 'i', 'you', 'it',
  'this', 'that', 'then', 'from', 'into', 'through', 'after', 'when',
])

/**
 * Goal token bag (ordered, de-duplicated, deterministic). Tokens shorter than
 * 3 chars are dropped along with stopwords.
 */
export function goalTokens(text: string): string[] {
  const seen = new Set<string>()
  const out: string[] = []
  for (const token of normalizeTokenText(text).split(' ')) {
    if (token.length < 3) continue
    if (GOAL_STOPWORDS.has(token)) continue
    if (seen.has(token)) continue
    seen.add(token)
    out.push(token)
  }
  return out
}

/**
 * Strip every provided data *value* from free-text so the goal key is
 * byte-equivalent for the same goal expressed with different secret values.
 * Slot names ({{...}}) are untouched. The `<data>` placeholder is
 * deterministic and value-independent.
 *
 * Safety policy (round-2 hardening):
 *   1. Values are canonicalized (trimmed, de-duplicated) and ordered by
 *      length DESCENDING (then locale) — never `Object.values()` insertion
 *      order — so overlapping values (e.g. "alpha" + "alphabeta") scrub the
 *      longest match first and never leave a shorter residue behind that could
 *      steer ranking.
 *   2. A single pass over the goal replaces all values at once. Placeholder
 *      groups are preserved; every other alternation match becomes `<data>`.
 *   3. Regex metacharacters are escaped for literal matching.
 *   4. Values 1–2 chars long are scrubbed ONLY as standalone words
 *      (`\b…\b`) and ONLY when purely word characters — so a short pin can
 *      never corrupt an unrelated common word ("xy" inside "oxygen"). Short
 *      values containing non-word characters are deliberately NOT scrubbed
 *      (fail-open, documented): scrubbing a 1-char punctuation secret would
 *      corrupt far more goal text than it protects, and such values are not
 *      realistic secrets.
 *   5. Matching is case-insensitive; numeric/boolean values are stringified.
 */
export function scrubDataValues(text: string, data: Record<string, string | number | boolean> | undefined): string {
  if (!data) return text
  const values = Array.from(
    new Set(
      Object.values(data)
        .map((v) => String(v).trim())
        .filter((v) => v.length > 0)
    )
  ).sort((a, b) => b.length - a.length || a.localeCompare(b))

  const longValues = values.filter((v) => v.length >= 3)
  const shortValues = values.filter((v) => v.length <= 2 && /^[A-Za-z0-9_]+$/.test(v))

  const parts: string[] = []
  if (longValues.length > 0) {
    parts.push(longValues.map((v) => v.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('|'))
  }
  if (shortValues.length > 0) {
    parts.push(
      shortValues
        .map((v) => `\\b(?:${v.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')})\\b`)
        .join('|')
    )
  }
  if (parts.length === 0) return text

  // Single pass: group 1 = preserved placeholder, group 2 = any scrubbed value.
  const re = new RegExp(`(\\{\\{\\s*[a-zA-Z0-9_.-]+\\s*\\}\\})|(${parts.join('|')})`, 'gi')
  return text.replace(re, (match, placeholder: string | undefined) =>
    placeholder != null ? placeholder : '<data>'
  )
}

/**
 * Detect a data VALUE that appears in the goal but cannot be scrubbed safely
 * (round-3 fail-closed). `scrubDataValues` deliberately fail-opens on 1–2 char
 * values containing non-word characters (scrubbing them would corrupt unrelated
 * goal text), so a goal embedding such a value would RETAIN it in the goal key —
 * and two different secrets could then steer flow selection. Detection mirrors
 * the normalization ACTUALLY used for ranking (`normalizeTokenText`): the goal
 * key collapses punctuation runs to spaces, so "use (a.)", "use a.," and "use a."
 * all normalize to the token "a" — exactly how the unscrubbable value "a."
 * normalizes. If that normalized value is a token of the normalized goal, the
 * compiler cannot tell the secret from a legitimate goal token and fails
 * CLOSED. Returns the DATA KEY whose value leaked (a diagnostic slot name,
 * never the secret value itself).
 */
export function unscrubbableGoalLeak(
  goal: string,
  data: Record<string, string | number | boolean> | undefined
): string | null {
  if (!data) return null
  const normTokens = new Set(
    normalizeTokenText(goal)
      .split(' ')
      .filter((t) => t.length > 0)
  )
  for (const key of Object.keys(data).sort()) {
    const v = String(data[key]).trim()
    if (v.length === 0 || v.length >= 3) continue // scrubbable (long)
    if (/^[A-Za-z0-9_]+$/.test(v)) continue // scrubbed as a standalone word
    const nv = normalizeTokenText(v)
    if (nv.length === 0) continue // drops out of the goal key entirely
    if (normTokens.has(nv)) return key // unscrubbable value in the normalized goal key
  }
  return null
}

// ---------------------------------------------------------------------------
// Intent vocabulary (bounded lexical; used by the goal matcher). This is a
// P3-owned projection, NOT an LLM call and NOT a raw transition payload.
// ---------------------------------------------------------------------------

export const INTENT_ALIASES: Record<string, string[]> = {
  login: ['log in', 'login', 'sign in', 'signin', 'sign into', 'authenticate', 'sign-in'],
  create: ['create', 'new record', 'register', 'create form', 'add a record'],
  edit: ['edit', 'update', 'modify'],
  delete: ['delete', 'remove', 'delete preview', 'request delete'],
  search: ['search', 'query', 'run a search'],
  filter: ['filter', 'apply a filter'],
  paginate: ['pagination', 'next page', 'go to the next page', 'paginated'],
  modal: ['modal', 'confirm the modal'],
  tab: ['tab', 'switch in-page tabs', 'in-page tabs', 'new tab', 'switch tabs'],
  newtab: ['open link in new tab', 'new tab'],
  iframe: ['iframe', 'embedded form', 'frame'],
  upload: ['upload', 'upload a file'],
  spa: ['spa', 'same-url', 'advance the same-url'],
  validation: ['validation', 'submit valid', 'valid data'],
  session: ['session', 'expires', 'expired', 'cookie', 'session cookie'],
  stale: ['stale', 'selector'],
  funnel: ['funnel', 'complete the funnel', 'funnel step'],
}

/** Intent keys whose aliases appear as substrings in the normalized text. */
export function matchIntents(text: string): string[] {
  const normalized = normalizeTokenText(text)
  const hits: string[] = []
  for (const [intent, aliases] of Object.entries(INTENT_ALIASES)) {
    if (aliases.some((alias) => normalized.includes(normalizeTokenText(alias)))) {
      hits.push(intent)
    }
  }
  return hits.sort()
}

// ---------------------------------------------------------------------------
// Time canonicalization (deterministic: parses supplied timestamps, never reads
// the wall clock — no Date.now() anywhere in the compiler).
// ---------------------------------------------------------------------------

/** Millisecond epoch for a supplied timestamp (Date or string). NaN → 0. */
export function timestampMs(value: Date | string | null | undefined): number {
  if (value instanceof Date) return Number.isFinite(value.getTime()) ? value.getTime() : 0
  if (typeof value === 'string') {
    const ms = Date.parse(value)
    return Number.isFinite(ms) ? ms : 0
  }
  return 0
}

// ---------------------------------------------------------------------------
// P3-owned derivation types (NOT part of the frozen contract).
// ---------------------------------------------------------------------------

/**
 * Pure, DB-free projection of one `action_state_transitions` row the compiler
 * can consume. The corpus/runner adapter reads PostgreSQL and maps rows onto
 * this shape; the compiler never receives a DB handle. All timestamps are
 * allowed as either Date or string and are canonicalized internally.
 */
export interface CompilerTransitionV01 {
  transitionId: string
  fromStateId: string | null
  toStateId: string | null
  fromScreenStateId: number | null
  toScreenStateId: number | null
  routeFamily: string | null
  requestedUrl: string | null
  finalUrl: string | null
  authMode: 'anonymous' | 'authenticated' | null
  description: string | null
  outcome: string | null
  score: number | null
  confidence: number | null
  chainDepth: number | null
  plannedActions: unknown
  plannedActionTemplate: unknown
  planningSource: string | null
  planningIntent: string | null
  planningSignals: string | null
  replaySuccessCount: number | null
  replayFailureCount: number | null
  lastReplayStatus: string | null
  lastReplayedAt: Date | string | null
  observedAt: Date | string | null
  /** Serialized template/actions byte length (for the payload cap). */
  payloadBytes?: number
}

/** Pure projection of a `screen_states` row (compiler never touches the DB). */
export interface CompilerScreenV01 {
  screenId: number
  stateKey: string
  url: string
  routeFamily: string | null
  title?: string | null
  authMode?: 'anonymous' | 'authenticated' | null
  confidence?: number | null
}

/** Pre-derived scenario candidate (C2). When absent, the compiler derives scenarios from transitions. */
export interface CompilerScenarioV01 {
  scenarioId: string
  scenarioVersion: string
  routeFamily: string
  transitionIds: string[]
  description?: string | null
  intent?: string | null
}

/** 12 compiler bounds. All deterministic caps — not SLOs. */
export interface CompilerLimitsV01 {
  /** goal characters before truncation */
  goalMaxChars: number
  /** transition projection pool cap */
  transitionsMax: number
  /** screen pool cap */
  screensMax: number
  /** derived scenario pool cap */
  scenariosMax: number
  /** longest allowed chain (hops) */
  pathMaxDepth: number
  /** chain candidates kept before ranking */
  pathCandidatesMax: number
  /** max required+optional data slots in the IR */
  dataSlotsMax: number
  /** max IR steps across the whole flow */
  stepsMaxTotal: number
  /** max template steps per transition */
  templateStepsMax: number
  /** max serialized bytes of a single transition's template/actions payload */
  transitionPayloadBytesMax: number
  /** max startUrl characters */
  urlCharsMax: number
  /** max request.data entries */
  requestDataEntriesMax: number
}

export const COMPILER_LIMITS: CompilerLimitsV01 = {
  goalMaxChars: 400,
  transitionsMax: 2000,
  screensMax: 2000,
  scenariosMax: 256,
  pathMaxDepth: 8,
  pathCandidatesMax: 64,
  dataSlotsMax: 16,
  stepsMaxTotal: 64,
  templateStepsMax: 32,
  transitionPayloadBytesMax: 8192,
  urlCharsMax: 2048,
  requestDataEntriesMax: 16,
}

/**
 * Normalize ONE compiler limit (P2 budget-style): non-number / NaN / ±Infinity
 * → default; fractional → floor; below `min` → `min`. Deterministic and
 * crash-proof — a NaN/±Infinity override can never leak into `slice`/loop math
 * or turn `ranked[0]` into a TypeError.
 */
export function normalizeCompilerLimit(value: number | undefined, fallback: number, min = 0): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) return fallback
  return Math.max(min, Math.floor(value))
}

/**
 * Normalize a partial limits override onto the frozen defaults. The compiler
 * ALWAYS works from this normalized object — never from a raw merge of caller
 * input — so `pathCandidatesMax: NaN`, `transitionsMax: Infinity`, negative or
 * fractional overrides all resolve deterministically to a valid bound.
 */
export function normalizeCompilerLimits(override?: Partial<CompilerLimitsV01>): CompilerLimitsV01 {
  const d = COMPILER_LIMITS
  return {
    goalMaxChars: normalizeCompilerLimit(override?.goalMaxChars, d.goalMaxChars),
    transitionsMax: normalizeCompilerLimit(override?.transitionsMax, d.transitionsMax),
    screensMax: normalizeCompilerLimit(override?.screensMax, d.screensMax),
    scenariosMax: normalizeCompilerLimit(override?.scenariosMax, d.scenariosMax),
    pathMaxDepth: normalizeCompilerLimit(override?.pathMaxDepth, d.pathMaxDepth, 1),
    pathCandidatesMax: normalizeCompilerLimit(override?.pathCandidatesMax, d.pathCandidatesMax, 1),
    dataSlotsMax: normalizeCompilerLimit(override?.dataSlotsMax, d.dataSlotsMax),
    stepsMaxTotal: normalizeCompilerLimit(override?.stepsMaxTotal, d.stepsMaxTotal),
    templateStepsMax: normalizeCompilerLimit(override?.templateStepsMax, d.templateStepsMax),
    transitionPayloadBytesMax: normalizeCompilerLimit(override?.transitionPayloadBytesMax, d.transitionPayloadBytesMax),
    urlCharsMax: normalizeCompilerLimit(override?.urlCharsMax, d.urlCharsMax),
    requestDataEntriesMax: normalizeCompilerLimit(override?.requestDataEntriesMax, d.requestDataEntriesMax),
  }
}

/** Compact, secret-free compiler telemetry (stable field set). */
export interface CompileTelemetryV01 {
  version: '0.1'
  candidateTransitions: number
  screensPool: number
  filteredStale: number
  filteredUnsafeTemplate: number
  filteredAuthIncompatible: number
  filteredOutcome: number
  /** candidates skipped for exceeding a hard bound (stepsMaxTotal / dataSlotsMax). */
  filteredBounds: number
  goalScore: number
  goalMatch: 'exact' | 'alias' | 'token' | 'family' | 'none'
  pathTier: 'scenario' | 'route_family' | 'graph_path' | 'none'
  hops: number
  chainTransitionIds: string[]
  requiredDataSlots: string[]
  optionalDataSlots: string[]
  sourceEvidence: ('scenario' | 'graph_path' | 'route_family' | 'fallback')[]
  reason?: 'no_scenario' | 'no_path' | 'missing_data' | 'screen_ambiguous' | 'contract_conflict'
  message?: string
  /** pool saturation dropped a candidate that could not be distinguished from the winner (fail-closed). */
  poolSaturatedAmbiguous?: boolean
  /** equal-strength candidates resolved to different destinations and no canonical dedupe applied (fail-closed). */
  equalStrengthAmbiguous?: boolean
  /** the winning chain references a transitionId that has conflicting duplicate payloads (fail-closed, never picks one). */
  duplicateIdConflict?: boolean
  /** the goal embeds an unscrubbable data value (raw goal token) that could steer flow selection (fail-closed). */
  secretGoalLeak?: boolean
  /** the screensMax cap masked a C4 destination ambiguity that the full pool would have caught (fail-closed). */
  screenCapMaskedAmbiguity?: boolean
}

/**
 * P3 compile outcome. `ok:true` yields the frozen IR; `ok:false` yields one
 * canonical P0 failure reason (subset of `BrowserTaskFailureReasonV01`).
 */
export type TaskCompileOutcomeV01 =
  | {
      ok: true
      ir: BrowserFlowIRV01
      telemetry: CompileTelemetryV01
    }
  | {
      ok: false
      reason: 'no_scenario' | 'no_path' | 'missing_data' | 'screen_ambiguous' | 'contract_conflict'
      missingDataSlots?: string[]
      discoveryRecommendation?: { recommended: true; reason: 'planner_no_path' }
      telemetry: CompileTelemetryV01
    }

/** What the compiler needs: the request, the P2 screen verdict, and pure projections. */
export interface TaskCompilerInputV01 {
  request: BrowserTaskRequestV01
  /** P2-resolved current screen. The actual screen wins over request.startUrl. */
  currentScreen: ScreenIdentityV01 | null
  /** Why currentScreen is null (P2 fail-closed reason), when it is. */
  screenFailure?: 'screen_ambiguous' | 'screen_unknown' | 'screen_stale' | null
  /** Pure candidate transitions (adapter reads the DB; compiler never gets a handle). */
  transitions: CompilerTransitionV01[]
  /** Optional screen pool for destination identity resolution. */
  screens?: CompilerScreenV01[]
  /** Optional pre-derived scenario candidates; derived from transitions when absent. */
  scenarios?: CompilerScenarioV01[]
  /** Bounds override (defaults when absent). */
  limits?: Partial<CompilerLimitsV01>
}

// ---------------------------------------------------------------------------
// Loose template-step projection (matches the production TemplateStep shape and
// the fixture templates). The compiler never copies raw tool args or whole
// transition payloads — only these whitelisted fields.
// ---------------------------------------------------------------------------

export interface TemplateStepV01 {
  kind: string
  target?: {
    scope?: string
    role?: string
    index?: number
    locator?: string
  }
  valueKind?: string
  /** Optional explicit slot reference when the template declares one. */
  slotName?: string
  /** Optional explicit timeout override. */
  timeoutMs?: number
  /**
   * Captured ONLY when the step's authored value is a compiler-owned
   * non-secret literal (e.g. 'submit', 'next'). Secrets and arbitrary values
   * are never captured — they must be slot references. Never inlined into the
   * IR except as a `literal_bound` binding.
   */
  value?: string
}

/** The 10 IR action kinds (frozen union). */
export const IR_ACTION_KINDS = [
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
] as const

export type IrActionKindV01 = (typeof IR_ACTION_KINDS)[number]

/** Template kinds map 1:1 onto IR action kinds (no invented kinds). */
export const TEMPLATE_TO_IR_ACTION: Record<string, IrActionKindV01> = {
  navigate: 'navigate',
  click: 'click',
  fill: 'fill',
  type: 'type',
  press: 'press',
  select: 'select',
  hover: 'hover',
  upload: 'upload',
  wait: 'wait',
  assert: 'assert',
}

/** Explicit per-action timeout (compiler-owned; deterministic). */
export const ACTION_TIMEOUT_MS: Record<IrActionKindV01, number> = {
  navigate: 15_000,
  click: 8_000,
  fill: 8_000,
  type: 8_000,
  press: 8_000,
  select: 8_000,
  hover: 8_000,
  upload: 8_000,
  wait: 1_000,
  assert: 5_000,
}

export type StepMutationClassV01 = 'read' | 'idempotent_mutation' | 'stateful_mutation'

/**
 * Deterministic mutation classification per template step. Conservative where
 * ambiguous: submit-like clicks are stateful so a non-idempotent mutation is
 * never retried by recovery (P6 rule).
 */
export function classifyTemplateStepMutation(step: TemplateStepV01): StepMutationClassV01 {
  const kind = step.kind
  const role = step.target?.role ?? ''
  const locator = step.target?.locator ?? ''
  switch (kind) {
    case 'click': {
      if (role === 'auth_entry' || role === 'submit_action') return 'stateful_mutation'
      if (/submit|save|delete|confirm|create|update|add|remove|next|finish/i.test(locator)) {
        return 'stateful_mutation'
      }
      return 'idempotent_mutation'
    }
    case 'fill':
    case 'type':
    case 'press':
    case 'select':
    case 'hover':
    case 'navigate':
      return 'idempotent_mutation'
    case 'upload':
      return 'stateful_mutation'
    case 'wait':
    case 'assert':
      return 'read'
    default:
      return 'read'
  }
}

/** Aggregate idempotency: any stateful → stateful; else any mutation → idempotent; else read. */
export function aggregateIdempotency(classes: StepMutationClassV01[]): 'read' | 'idempotent_mutation' | 'stateful_mutation' {
  if (classes.includes('stateful_mutation')) return 'stateful_mutation'
  if (classes.includes('idempotent_mutation')) return 'idempotent_mutation'
  return 'read'
}

/** Recovery policy mapping: read may retry; mutations never retry by default. */
export function recoveryPolicyForIdempotency(
  idempotency: 'read' | 'idempotent_mutation' | 'stateful_mutation'
): 'none' | 'single_retry_read' | 'bounded' {
  return idempotency === 'read' ? 'single_retry_read' : 'none'
}

// ---------------------------------------------------------------------------
// Auth helpers (C1-derived, deterministic).
// ---------------------------------------------------------------------------

/** C1: current session authMode ← edge auth_mode → runtime session → anonymous. */
export function deriveSessionAuthMode(screen: ScreenIdentityV01 | CompilerScreenV01 | null): AuthModeV01 {
  if (screen?.authMode === 'authenticated') return 'authenticated'
  return 'anonymous'
}

/** A chain's required auth: authenticated iff any edge requires it. */
export function chainAuthMode(transitions: readonly CompilerTransitionV01[]): AuthModeV01 {
  return transitions.some((t) => t.authMode === 'authenticated') ? 'authenticated' : 'anonymous'
}

/** A transition performs authentication iff its template carries a secret step. */
export function isLoginTransition(t: CompilerTransitionV01): boolean {
  const template = parseTemplatePayload(t) ?? []
  return template.some(
    (step) =>
      step.valueKind === 'valid_secret' ||
      step.valueKind === 'invalid_secret' ||
      step.valueKind === 'mismatched_secret' ||
      step.target?.role === 'secret_field'
  )
}

/** Heuristic: does this text describe a login/public-auth surface? */
export function isLoginSurface(text: string): boolean {
  const normalized = normalizeTokenText(text)
  return (
    normalized.includes('/login') ||
    normalized.includes(' login ') ||
    normalized.includes('sign-in') ||
    normalized.includes('signin') ||
    normalized.includes('sign-in') ||
    normalized.includes('log-in')
  )
}

/**
 * Auth-compatibility filter (the spec's "incompatible-auth-without-supported-
 * transition"). A chain is incompatible only when an anonymous session must
 * reach an authenticated destination with no in-band way to authenticate:
 * no login/credential transition inside the chain and the chain does not end
 * at a public (login) surface.
 */
export function isAuthCompatible(args: {
  currentAuthMode: AuthModeV01
  chain: CompilerTransitionV01[]
  finalDestinationText: string
}): boolean {
  if (args.currentAuthMode === 'authenticated') return true
  const hasAuthenticatedEdge = args.chain.some((t) => t.authMode === 'authenticated')
  if (!hasAuthenticatedEdge) return true
  // The chain authenticates in-band (login transition with credentials).
  if (args.chain.some((t) => isLoginTransition(t))) return true
  // The chain exits to a public/login surface (e.g. session expiry → login).
  if (isLoginSurface(args.finalDestinationText)) return true
  return false
}

// ---------------------------------------------------------------------------
// Value/slot binding helpers.
// ---------------------------------------------------------------------------

export type StepValueClassV01 = 'secret' | 'search_query' | 'file' | 'email' | 'text' | 'toggle' | 'none'

/** Classify what a template step's value represents (for slot binding). */
export function classifyStepValue(step: TemplateStepV01): StepValueClassV01 {
  const kind = step.kind
  const vk = step.valueKind ?? 'field_default'
  const role = step.target?.role ?? ''
  if (kind === 'click' || kind === 'hover' || kind === 'wait' || kind === 'assert' || kind === 'navigate' || kind === 'press') {
    return 'none'
  }
  if (vk === 'valid_secret' || vk === 'invalid_secret' || vk === 'mismatched_secret') return 'secret'
  if (role === 'secret_field') return 'secret'
  if (vk === 'search_query' || role === 'search_query') return 'search_query'
  if (role === 'file_field') return 'file'
  if (role === 'toggle_field') return 'toggle'
  if (role === 'email_field' || role.includes('email')) return 'email'
  return 'text'
}

/** Deterministic hint regexes per value class, tried in order. */
const SLOT_HINT_REGEXES: Record<StepValueClassV01, RegExp[]> = {
  secret: [/pass|pwd|secret|token/i],
  search_query: [/search|query|\bq\b/i],
  file: [/file|upload/i],
  email: [/email|mail/i],
  text: [/email|mail/i, /name|title|item|label/i],
  toggle: [],
  none: [],
}

/**
 * Bind a template value step to a provided slot. Deterministic: hint-match
 * first (canonical order among ties), then first remaining canonical slot.
 * Returns the slot name or null when no provided slot fits.
 */
export function pickSlotForStep(
  step: TemplateStepV01,
  providedSlots: readonly string[],
  consumed: ReadonlySet<string>,
  canonicalOrder: readonly string[]
): string | null {
  const explicit = step.slotName?.trim()
  if (explicit) return explicit
  const cls = classifyStepValue(step)
  if (cls === 'none' || cls === 'toggle') return null
  const available = canonicalOrder.filter((slot) => !consumed.has(slot))
  const hints = SLOT_HINT_REGEXES[cls]
  for (const hint of hints) {
    const hit = available.find((slot) => hint.test(slot))
    if (hit) return hit
  }
  if (providedSlots.length === 0) return null
  const first = available[0] ?? null
  return first
}

/** Canonical data slot names: present names sorted; optional slots are the remaining data entries. */
export function canonicalSlotNames(
  required: readonly string[],
  provided: Record<string, string | number | boolean> | undefined
): { requiredSlots: string[]; optionalSlots: string[] } {
  const requiredSet = new Set(required)
  const requiredSorted = [...requiredSet].sort()
  const providedNames = Object.keys(provided ?? {}).sort()
  const optionalSorted = providedNames.filter((name) => !requiredSet.has(name))
  return { requiredSlots: requiredSorted, optionalSlots: optionalSorted }
}

// ---------------------------------------------------------------------------
// Node identity (port of the graph-chain transitionNode convention).
// ---------------------------------------------------------------------------

/** Deterministic node id for a screen (numeric screenId), state (stateKey), or url. */
export function nodeId(kind: 'screen' | 'state' | 'url', value: string | number | null): string | null {
  if (value === null || value === undefined || value === '') return null
  return `${kind}:${typeof value === 'number' ? String(value) : value}`
}

/** Start nodes a transition can join the graph from, in priority order. */
export function transitionStartNodes(t: CompilerTransitionV01): string[] {
  return [
    nodeId('screen', t.fromScreenStateId),
    nodeId('state', t.fromStateId),
    nodeId('url', t.requestedUrl ? normalizeRouteKey(t.requestedUrl) : null),
  ].filter((n): n is string => n !== null)
}

/** End nodes a transition reaches. */
export function transitionEndNodes(t: CompilerTransitionV01): string[] {
  return [
    nodeId('screen', t.toScreenStateId),
    nodeId('state', t.toStateId),
    nodeId('url', t.finalUrl ? normalizeRouteKey(t.finalUrl) : null),
  ].filter((n): n is string => n !== null)
}

/** Nodes the current screen matches (for start-node resolution). */
export function screenMatchNodes(screen: ScreenIdentityV01 | CompilerScreenV01 | null): string[] {
  if (!screen) return []
  const stateKey = 'stateKey' in screen ? screen.stateKey : ''
  const url = 'url' in screen ? screen.url : ''
  const screenId = 'screenId' in screen ? screen.screenId : undefined
  return [
    nodeId('state', stateKey),
    nodeId('screen', screenId ?? null),
    nodeId('url', url ? normalizeRouteKey(url) : null),
  ].filter((n): n is string => n !== null)
}

// ---------------------------------------------------------------------------
// Template payload parsing (pure; never copies raw tool args or whole payloads).
// ---------------------------------------------------------------------------

/**
 * TRUE number of template steps, computed against the raw payload before any
 * cap is applied. Used to fail closed: a template with more steps than
 * `templateStepsMax` is REJECTED outright (never silently truncated, never
 * parsed as `limit` steps).
 */
export function rawTemplateStepCount(t: CompilerTransitionV01): number {
  let raw: unknown = t.plannedActionTemplate ?? t.plannedActions
  if (typeof raw === 'string') {
    try {
      raw = JSON.parse(raw)
    } catch {
      return 0
    }
  }
  if (!Array.isArray(raw)) return 0
  return raw.length
}

/**
 * Parse a transition's template into the loose projection. Handles both a
 * JSON string and an already-parsed array. Caps steps at the limit. Any parse
 * failure yields `null` (the compiler treats it as an unsafe template).
 */
export function parseTemplatePayload(t: CompilerTransitionV01, limit = COMPILER_LIMITS.templateStepsMax): TemplateStepV01[] | null {
  let raw: unknown = t.plannedActionTemplate ?? t.plannedActions
  if (typeof raw === 'string') {
    try {
      raw = JSON.parse(raw)
    } catch {
      return null
    }
  }
  if (!Array.isArray(raw)) return null
  const steps: TemplateStepV01[] = []
  for (let i = 0; i < Math.min(raw.length, limit); i++) {
    const item = raw[i] as Record<string, unknown> | undefined
    if (!item || typeof item.kind !== 'string') return null
    const target = item.target
    const step: TemplateStepV01 = {
      kind: item.kind,
      target:
        target && typeof target === 'object'
          ? {
              scope: typeof (target as Record<string, unknown>).scope === 'string' ? String((target as Record<string, unknown>).scope) : undefined,
              role: typeof (target as Record<string, unknown>).role === 'string' ? String((target as Record<string, unknown>).role) : undefined,
              index: typeof (target as Record<string, unknown>).index === 'number' ? Number((target as Record<string, unknown>).index) : undefined,
              locator: typeof (target as Record<string, unknown>).locator === 'string' ? String((target as Record<string, unknown>).locator) : undefined,
            }
          : undefined,
      valueKind: typeof item.valueKind === 'string' ? item.valueKind : undefined,
      slotName: typeof item.slotName === 'string' ? item.slotName : undefined,
      timeoutMs: typeof item.timeoutMs === 'number' ? item.timeoutMs : undefined,
    }
    // Capture a compiler-owned non-secret literal only. Secrets and arbitrary
    // values never enter the projection (they must be slot references).
    if (typeof item.value === 'string') {
      const value = item.value.trim()
      if (value && isCompilerOwnedLiteral(value)) step.value = value
    }
    steps.push(step)
  }
  return steps
}

/**
 * A template step carries an inlined secret literal iff it is a secret-kind
 * step with a non-empty `value` that is NOT a `{{slot}}` placeholder. The
 * compiler rejects those (secrets are only ever slot references).
 */
export function templateHasSecretLiteral(t: CompilerTransitionV01): boolean {
  const raw = t.plannedActionTemplate ?? t.plannedActions
  let items: unknown[] = []
  if (typeof raw === 'string') {
    try {
      items = JSON.parse(raw)
    } catch {
      return false
    }
  } else if (Array.isArray(raw)) {
    items = raw
  }
  return items.some((entry) => {
    if (!entry || typeof entry !== 'object') return false
    const item = entry as Record<string, unknown>
    const vk = typeof item.valueKind === 'string' ? item.valueKind : ''
    const role =
      item.target && typeof item.target === 'object'
        ? String((item.target as Record<string, unknown>).role ?? '')
        : ''
    const isSecretKind =
      vk === 'valid_secret' || vk === 'invalid_secret' || vk === 'mismatched_secret' || role === 'secret_field'
    if (!isSecretKind) return false
    const value = typeof item.value === 'string' ? item.value.trim() : ''
    if (!value) return false
    return !/\{\{\s*[a-zA-Z0-9_.-]+\s*\}\}/.test(value)
  })
}

/** Serialized byte length of a transition's template/actions payload (for the cap). */
export function transitionPayloadBytes(t: CompilerTransitionV01): number {
  const raw = t.plannedActionTemplate ?? t.plannedActions
  if (typeof raw === 'string') return Buffer.byteLength(raw, 'utf8')
  try {
    return Buffer.byteLength(JSON.stringify(raw), 'utf8')
  } catch {
    return 0
  }
}

/** Deterministic canonical projection parts for a template step (secret-free). */
export function stepProjectionParts(step: TemplateStepV01): string[] {
  return [
    step.kind,
    step.target?.scope ?? '',
    step.target?.role ?? '',
    step.target?.index !== undefined ? String(step.target.index) : '',
    step.target?.locator ?? '',
    step.valueKind ?? '',
    step.slotName ?? '',
  ]
}

// ---------------------------------------------------------------------------
// Required-slot extraction (port of graph-chain `requiredSlots` / `explicitDataSlots`,
// with the missing-slot presence check switched from truthiness to key membership).
// ---------------------------------------------------------------------------

/** `{{ slot_name }}` placeholders anywhere in a text blob (canonical names). */
export function extractPlaceholders(text: string): string[] {
  const names = new Set<string>()
  const re = /\{\{\s*([a-zA-Z0-9_.-]+)\s*\}\}/g
  let m: RegExpExecArray | null
  while ((m = re.exec(text)) !== null) {
    names.add(m[1])
  }
  return [...names].sort()
}

/** dataSlots[{name, required!==false}] + guards[{kind:'slot_present',slot}] from one template item. */
export function explicitRequiredSlots(item: unknown): string[] {
  if (!item || typeof item !== 'object') return []
  const record = item as Record<string, unknown>
  const names = new Set<string>()
  const dataSlots = Array.isArray(record.dataSlots) ? record.dataSlots : []
  for (const entry of dataSlots) {
    if (entry && typeof entry === 'object') {
      const slot = entry as Record<string, unknown>
      if (typeof slot.name === 'string' && slot.name.trim()) {
        if (slot.required !== false) names.add(slot.name.trim())
      }
    }
  }
  const guards = Array.isArray(record.guards) ? record.guards : []
  for (const guard of guards) {
    if (guard && typeof guard === 'object') {
      const g = guard as Record<string, unknown>
      if (g.kind === 'slot_present' && typeof g.slot === 'string' && g.slot.trim()) names.add(g.slot.trim())
    }
  }
  return [...names].sort()
}

/**
 * Declared required slots for a transition: `{{placeholders}}` in the serialized
 * template + explicit dataSlots/guards on each step. This is the graph-chain
 * `requiredSlots` port. Deterministic, secret-free (only slot NAMES appear).
 */
export function transitionRequiredSlots(t: CompilerTransitionV01, limit = COMPILER_LIMITS.templateStepsMax): string[] {
  const raw = t.plannedActionTemplate ?? t.plannedActions
  const text = JSON.stringify(raw ?? '')
  const names = new Set(extractPlaceholders(text))
  const items = Array.isArray(raw) ? raw.slice(0, limit) : []
  for (const item of items) {
    for (const name of explicitRequiredSlots(item)) names.add(name)
  }
  return [...names].sort()
}

/**
 * P3-compatible presence check: a slot is "present" iff its name is a key in
 * request.data — 0 / false / "" all count. No generic truthiness. (Spec: "0/
 * false/explicitly-permitted-empty-string count as present".)
 */
export function slotPresent(requestData: Record<string, string | number | boolean> | undefined, slot: string): boolean {
  return requestData !== undefined && Object.prototype.hasOwnProperty.call(requestData, slot)
}

/** Missing required data slots, sorted, key-membership based. */
export function missingRequiredDataSlots(
  requestData: Record<string, string | number | boolean> | undefined,
  required: readonly string[]
): string[] {
  return required.filter((slot) => !slotPresent(requestData, slot)).sort()
}

// ---------------------------------------------------------------------------
// Compiler-owned literals (safe to bind as literal_bound; never secrets).
// ---------------------------------------------------------------------------

/** Literal strings the compiler itself may emit (locator-derived, non-secret). */
export const COMPILER_OWNED_LITERALS: ReadonlySet<string> = new Set([
  'true',
  'false',
  'checkbox',
  'radio',
  'submit',
  'save',
  'delete',
  'confirm',
  'next',
  'finish',
  'open',
  'close',
  'new-tab',
  'iframe',
])

/** A literal is compiler-owned only if it is one of the frozen allowed tokens. */
export function isCompilerOwnedLiteral(value: string): boolean {
  return COMPILER_OWNED_LITERALS.has(value.trim())
}

// ---------------------------------------------------------------------------
// Node-identity complement: single-node ports of graph-chain `transitionNode`.
// ---------------------------------------------------------------------------

/**
 * Graph-chain `transitionNode` port: the single highest-priority node a
 * transition joins/leaves the graph at (screen → state → url). Used for BFS
 * adjacency; start matching uses the multi-node candidates.
 */
export function transitionFromNode(t: CompilerTransitionV01): string | null {
  return transitionStartNodes(t)[0] ?? null
}

export function transitionToNode(t: CompilerTransitionV01): string | null {
  return transitionEndNodes(t)[0] ?? null
}

/** Does this transition start at the given screen (any candidate node)? */
export function transitionStartsAtScreen(t: CompilerTransitionV01, screen: ScreenIdentityV01 | CompilerScreenV01 | null): boolean {
  if (!screen) return false
  const screenNodes = new Set(screenMatchNodes(screen))
  return transitionStartNodes(t).some((node) => screenNodes.has(node))
}

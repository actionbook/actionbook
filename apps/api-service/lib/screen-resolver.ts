/**
 * Deterministic Screen Resolver (P2).
 *
 * Identifies the current functional screen / state-variant from a BOUNDED,
 * structured runtime snapshot — WITHOUT any LLM DOM-classification call —
 * by ranking stored screen candidates against five deterministic evidence
 * tiers and failing closed on ambiguity.
 *
 * This module is pure: no DB, no daemon, no extension bridge, no Zod, no
 * network. The adapter that collects the snapshot and the adapter that loads
 * stored candidates from `screen_states` live OUTSIDE this module (the runtime
 * `locate_screen` lookup is untouched; discovery's warm-cache hydration is a
 * separate layer). Every list/string the resolver touches is bounded here,
 * before scoring — never a generic truncate of an already-collected DOM.
 *
 * Candidate bounding (H1): BEFORE scoring, a deterministic cheap-evidence
 * shortlist is built from canonical URL (kept first, up to the budget), route-
 * family, and a stable semantic/auth fallback allocation up to `maxCandidates`
 * (default `RESOLVER_LIMITS.maxCandidates`). The shortlist is HARD-bounded —
 * `shortlist.length <= normalizedBudget` for EVERY input (priority tiers are
 * capped, not exempted). A priority tier that exceeds the budget is reported
 * SATURATED and fails closed as `screen_ambiguous` (shortlist_saturated_url /
 * shortlist_saturated_route) unless the kept winner is a decisive exact-
 * fingerprint match with no dropped same-fingerprint twin. Truncation is
 * reported via `telemetry.shortlistDropped` and the `shortlist_truncated` rule.
 * The fallback pool is cheap-pre-ranked by a token-overlap upper bound (NO full
 * semantic scan of the pool); weighted Jaccard is computed only on the budgeted
 * prefiltered rows.
 *
 * Resolution order (method-aware, deterministic):
 *   1. exact compatible fingerprint            (url + title + variant + auth + semantic sets)
 *   2. exact canonical URL + route-family + state-variant evidence
 *   3. strong live-ref / interactive-structure match (ONLY when the live
 *      snapshot actually carries interactive-ref evidence — a page whose
 *      affordance surface is pure landmarks is matched by SEMANTIC fallback,
 *      not a "live-ref" claim)
 *   4. semantic weighted-Jaccard fallback       (the existing `rankScreens` path)
 *   5. stable tie-break independent of DB row order (stateKey, url, screenId)
 *
 * Fail-closed rules (P0 §5):
 *   - best confidence below `minConfidence`            -> screen_ambiguous
 *   - top-1 and top-2 in the same evidence tier within the
 *     undifferentiated band, without a decisive fingerprint  -> screen_ambiguous
 *     (unless a decisive TITLE elects exactly one of the tied rows — R6F2;
 *      see `titleDecisiveCandidate` for the fail-closed conditions)
 *   - two DISTINCT `screen_states` rows share the exact fingerprint
 *     (numeric id collision — C4)                      -> screen_ambiguous
 *   - conflicting auth/state/fingerprint signals            -> screen_ambiguous or screen_stale, never a guess
 *   - no candidates at all                                  -> screen_unknown
 *   - exact URL but structure fundamentally changed         -> screen_stale (structure_changed)
 *   - best fingerprint evidence is STALE (candidate stored under an
 *     older cache version)                                 -> screen_stale (stale_fingerprint drift rejection)
 *   - cache version expected but the candidate row carries NONE
 *     (H3 fail-closed — missing is never "fresh")           -> screen_stale (missing_cache_version)
 *
 * A weak exact-URL match NEVER defeats a contradictory fingerprint or
 * state-variant (the URL-matching candidate is demoted/excluded when its
 * stored structure fundamentally disagrees with the live structure).
 */

import {
  buildSignature,
  computeScreenFingerprint,
  fieldJaccard,
  redactHeadingText,
  routeFamilyMatch,
  weightedJaccard,
  type ScreenFingerprintInput,
  type SemanticSignature,
  type SignatureWeights,
  DEFAULT_SIGNATURE_WEIGHTS,
} from './screen-signature';
import type { AuthModeV01, ScreenIdentityV01 } from '../types/browser-task';

// ---------------------------------------------------------------------------
// Bounds — applied defensively to the structured snapshot BEFORE any scoring.
// Collection-side bounding is the adapter's job; these are the resolver's own
// caps so a malformed/oversized snapshot can never inflate a signature.
// ---------------------------------------------------------------------------

export const RESOLVER_LIMITS = {
  maxTitle: 200,
  maxHeadings: 24,
  maxHeadingLen: 160,
  maxLandmarks: 24,
  maxLandmarkLen: 96,
  maxForms: 16,
  maxFieldsPerForm: 16,
  maxFieldLen: 48,
  maxRefs: 48,
  maxRefRoleLen: 32,
  maxRefLabelLen: 120,
  maxTabs: 16,
  maxTabLen: 64,
  maxIframeIdentityLen: 128,
  maxUrlLen: 2048,
  /**
   * Default candidate budget for the cheap-evidence shortlist (H1). The shortlist
   * is built BEFORE scoring and is HARD-bounded: `shortlist.length <= budget`
   * for EVERY input. URL-exact rows are kept first (up to the budget), then
   * route-family matches, then a deterministic fallback allocation. A priority
   * tier with more rows than the budget can hold is reported SATURATED and the
   * pool fails closed as `screen_ambiguous` (shortlist_saturated_url /
   * shortlist_saturated_route) unless the kept winner is a decisive exact-
   * fingerprint match with no dropped same-fingerprint twin. Truncation is
   * reported via `shortlistDropped` / the `shortlist_truncated` rule.
   */
  maxCandidates: 100,
  /**
   * Fallback prefilter slack (H1): the rest pool is cheap-pre-ranked by a
   * deterministic token-overlap upper bound (NO full semantic scan), the top
   * `fallbackBudget + prefilterSlack` rows are full-scored, then trimmed to the
   * fallback budget. This bounds full weighted-Jaccard work independent of pool
   * size. A tier-0 exact-fingerprint row always holds the maximal cheap key, so
   * the prefilter can never drop an exact winner.
   */
  prefilterSlack: 32,
  maxMatches: 8,
} as const;

/**
 * Normalize a caller-supplied candidate budget to a safe, non-negative integer
 * (H1 hard-bound input sanitation):
 *   - non-number / NaN / ±Infinity  -> `fallback` (the documented default)
 *   - decimal                       -> floor (3.7 -> 3)
 *   - negative                      -> 0 (score nothing -> fail closed)
 *   - zero                          -> 0
 * A normalized budget of 0 scores no candidates and yields `screen_unknown`
 * (fail closed — never a guess on a nonsensical budget).
 */
export function normalizeBudget(value: unknown, fallback: number): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) return fallback;
  const n = Math.floor(value);
  return n < 0 ? 0 : n;
}

// ---------------------------------------------------------------------------
// Thresholds (documented, testable).
// ---------------------------------------------------------------------------

/** Min confidence for a candidate to be a resolved screen (P0 §5). */
export const DEFAULT_MIN_CONFIDENCE = 0.55;

/** Top-1 vs top-2 gap below which two same-tier candidates are undifferentiated. */
export const DEFAULT_AMBIGUITY_BAND = 0.12;

/**
 * Structural-conflict floor: when the live canonical URL matches a candidate's
 * URL but the semantic similarity is BELOW this floor, the candidate's stored
 * structure fundamentally disagrees with the live structure ("exact URL
 * contradicted by live structure") — it is never selected confidently and
 * drives a `structure_changed` stale outcome when it is the only URL match.
 */
export const STRUCTURE_CONFLICT_FLOOR = 0.45;

/** Live-ref/interactive-structure match is "strong" at/above this score. */
export const LIVE_REF_STRONG = 0.65;

/**
 * Tier-2 (strong live-ref match) is only claimable when the LIVE snapshot
 * actually carries interactive-ref evidence. When `interactiveRefs` is empty,
 * `liveRefScore` degenerates to pure landmark overlap — and every non-trivial
 * page shares the same `nav`/`main`/`banner` landmarks, so landmark-only
 * overlap would rank dozens of unrelated screens as "strong live-ref matches"
 * at identical confidence and force a false `screen_ambiguous`. The semantic
 * tier discriminates those screens (by heading/structure); the live-ref tier
 * must not swallow them. See test/screen-resolver-corpus.test.ts.
 */
const LIVE_REF_REQUIRES_REFS = true;

/** Tier-1 URL winners with semantic below this are flagged structure_changed. */
export const STRUCTURE_DRIFT_CEIL = 0.85;

/** Evidence tiers (0 = strongest). */
export type EvidenceTier = 0 | 1 | 2 | 3;

// ---------------------------------------------------------------------------
// Input contracts.
// ---------------------------------------------------------------------------

/** One observed form: its bounded field-type set. */
export interface ResolverFormObserved {
  fieldTypes: string[];
}

/** One bounded semantic interactive ref (role + optional label). */
export interface ResolverInteractiveRef {
  role: string;
  label?: string;
}

/**
 * Same-URL SPA state-variant signals. Absent field = no such signal (the base
 * variant). `modal` carries a bounded kind (e.g. 'open:settings'); `tab`
 * carries the selected tab id; `iframe` carries an identity token (never DOM).
 */
export interface StateVariantV01 {
  modal?: string | null;
  tab?: string | null;
  iframe?: string | null;
}

/**
 * Bounded, structured runtime observation of the current screen. The adapter
 * that materializes this snapshot must bound collection itself; the resolver
 * re-bounds defensively below.
 */
export interface ResolverSnapshotV01 {
  url: string;
  title?: string | null;
  headings?: string[];
  landmarkRoles?: string[];
  forms?: ResolverFormObserved[];
  interactiveRefs?: ResolverInteractiveRef[];
  modal?: { open: boolean; kind?: string };
  tabPanel?: { selected: string; tabs: string[] };
  iframe?: { present: boolean; identity?: string };
  /** C1 runtime session auth signal ('authenticated' | 'anonymous' | null = unknown). */
  authSignal?: AuthModeV01 | null;
}

/** A stored screen_states candidate (adapter-provided). */
export interface StoredScreenV01 {
  /** Numeric `screen_states.id` — the identity (P0 C4). */
  screenId: number;
  /** `screen_states.stateId` — diagnostic, NOT unique. */
  stateKey: string;
  url: string;
  routeFamily?: string | null;
  title?: string | null;
  /** Same-URL SPA discriminator; absent = base variant. */
  stateVariant?: StateVariantV01 | null;
  /** Stored semantic signature (from `semanticSignature` backfill, else derived). */
  signature?: SemanticSignature | null;
  /** Derived auth mode per C1 (from action_state_transitions auth_mode). */
  storedAuthMode?: AuthModeV01 | null;
  /** Stored semanticSignature backfill/schema version. */
  cacheVersion?: string | null;
}

export interface ResolveScreenOptionsV01 {
  minConfidence?: number;
  ambiguityBand?: number;
  weights?: SignatureWeights;
  /** Live route/path family, when the runtime knows it (e.g. from URL parsing). */
  routeFamily?: string | null;
  /** Current/live semanticSignature cache version. */
  currentCacheVersion?: string | null;
  /**
   * Candidate budget for the cheap-evidence shortlist (H1). Default:
   * `RESOLVER_LIMITS.maxCandidates` (bounded). The shortlist is HARD-bounded
   * (`shortlist.length <= normalizedBudget` for every input): URL-exact rows
   * are kept first (up to the budget), then route-family matches, then a
   * deterministic fallback allocation. A priority tier that exceeds the budget
   * fails closed as `screen_ambiguous` unless the kept winner is a decisive
   * exact-fingerprint match with no dropped same-fingerprint twin. Invalid
   * budgets are normalized: NaN/±Infinity -> default, decimal -> floor,
   * negative -> 0. Raise this to trade boundedness for recall on very large
   * pools.
   */
  maxCandidates?: number;
  /** Max ranked matches returned (bounded). */
  maxMatches?: number;
}

// ---------------------------------------------------------------------------
// Output contracts.
// ---------------------------------------------------------------------------

export interface RankedResolution {
  screenId: number;
  stateKey: string;
  url: string;
  routeFamily?: string | null;
  stateVariant?: string;
  confidence: number;
  matchedBy: ScreenIdentityV01['matchedBy'];
  tier: EvidenceTier;
  semanticScore: number;
  liveRefScore: number;
  fingerprintExact: boolean;
  authConflict: boolean;
  structureConflict: boolean;
  /**
   * The candidate's proven origin differs from the live proven origin (both
   * sides provable, WHATWG `scheme://host[:port]`). Such a row is excluded from
   * eligibility at EVERY tier: the semantic/route tiers are origin-agnostic by
   * design, so without this gate a screen learned on `http://h:443` resolved —
   * and its transition ran — on `http://h` and vice versa.
   */
  originConflict: boolean;
  /**
   * R7 (round 7): the LIVE url proves an origin but this candidate CANNOT — a
   * legacy path-only row (`/login`), an empty url, junk, a non-http scheme.
   * `originConflict` needs both sides provable, so it could never see these
   * rows; the semantic and live-ref tiers are origin-agnostic, so a `/login`
   * row learned on `http://127.0.0.1:8797` resolved on `:8798` at tier 2
   * (measured confidence 0.8125, `matched_by=live_ref,semantic_signature`).
   * That is the same cross-origin hole as `originConflict`, reached through
   * unprovable data instead of a proven mismatch, so it fails closed the same
   * way. A row whose origin is unprovable because the LIVE url is unprovable
   * too is NOT flagged — nothing can be compared, which is the pre-gate
   * behavior.
   */
  originUnprovable: boolean;
  cacheStale: boolean;
  /** H3 fail-closed signal: a cache version is expected (`currentCacheVersion`
   *  set) but the candidate row carries none → treated as stale, not fresh. */
  cacheUnversioned: boolean;
  /** C1 derived auth mode from the transition/candidate row (first in the
   *  precedence chain: transition -> runtime session signal -> anonymous). */
  storedAuthMode?: AuthModeV01 | null;
  /** The candidate's ACTUAL stored cacheVersion (not the live current one). */
  cacheVersion?: string | null;
}

/**
 * Internal scored candidate: the public output shape PLUS the deterministic
 * identity fingerprint, used ONLY for C4 collision detection inside
 * resolveScreen. The fingerprint is never part of the public output
 * (matches / screen / telemetry) — it stays internal per the privacy contract.
 */
interface ScoredCandidate extends RankedResolution {
  fingerprint: string;
  /**
   * R6F2: the candidate's BOUNDED stored title, kept internal (never published
   * through `toPublicMatch`) and read only by `titleDecisiveCandidate`.
   */
  title: string;
}

/** Strip the internal fingerprint from a scored candidate for public output. */
function toPublicMatch(s: ScoredCandidate): RankedResolution {
  return {
    screenId: s.screenId,
    stateKey: s.stateKey,
    url: s.url,
    routeFamily: s.routeFamily ?? null,
    stateVariant: s.stateVariant,
    confidence: s.confidence,
    matchedBy: s.matchedBy,
    tier: s.tier,
    semanticScore: s.semanticScore,
    liveRefScore: s.liveRefScore,
    fingerprintExact: s.fingerprintExact,
    authConflict: s.authConflict,
    structureConflict: s.structureConflict,
    originConflict: s.originConflict,
    originUnprovable: s.originUnprovable,
    cacheStale: s.cacheStale,
    cacheUnversioned: s.cacheUnversioned,
    storedAuthMode: s.storedAuthMode ?? null,
    cacheVersion: s.cacheVersion ?? null,
  };
}

/** Top-N public matches from a scored list (bounded, deterministic order). */
function topMatches(sorted: readonly ScoredCandidate[], maxMatches: number): RankedResolution[] {
  return sorted.slice(0, Math.max(1, maxMatches)).map(toPublicMatch);
}

export type ScreenResolutionStatus =
  | 'resolved'
  | 'ambiguous'
  | 'unknown'
  | 'stale';

export type ScreenResolutionReason =
  | 'screen_ambiguous'
  | 'screen_unknown'
  | 'screen_stale';

export interface ScreenResolutionV01 {
  status: ScreenResolutionStatus;
  /** Canonical failure reason when status !== 'resolved'. */
  reason?: ScreenResolutionReason;
  screen: ScreenIdentityV01 | null;
  matches: RankedResolution[];
  telemetry: {
    /** True when the snapshot exceeded a bound and was defensively truncated. */
    inputTruncated: boolean;
    /** Approximate JSON byte size of the bounded snapshot (structure only). */
    inputByteSize: number;
    /** Number of candidates actually scored (the bounded shortlist size). */
    candidateCount: number;
    /** Full candidate-pool size BEFORE the cheap-evidence shortlist (H1). */
    poolTotal: number;
    /** The applied candidate budget (options.maxCandidates ?? default). */
    shortlistBudget: number;
    /** Rows kept by the shortlist (== candidateCount). */
    shortlistKept: number;
    /** Rows dropped by the shortlist (poolTotal - shortlistKept). */
    shortlistDropped: number;
    /** Cheap-evidence tier counts inside the shortlist (H1). */
    shortlistByUrl: number;
    shortlistByRoute: number;
    shortlistByFallback: number;
    /** Priority tier that exceeded the budget (H1 hard bound), if any. */
    shortlistSaturatedTier: 'url' | 'route' | 'url,route' | null;
    /** Ordered evidence tiers/signals actually applied. */
    rules: string[];
    durationMs: number;
  };
}

// ---------------------------------------------------------------------------
// Canonical URL normalization.
// ---------------------------------------------------------------------------

/**
 * Canonicalize a URL for identity comparison: strip fragment, lowercase host,
 * drop the port that is DEFAULT FOR THE SCHEME, strip trailing slashes, sort
 * query keys. Non-parseable strings degrade to a minimal deterministic
 * normalization. Returns '' for empty input.
 *
 * Port dropping is WHATWG-correct, not scheme-blind: only `http:80` and
 * `https:443` collapse onto their bare hosts. Stripping 80/443 unconditionally
 * (the previous behavior) aliased REAL origins — `http://h:443` and
 * `https://h:80` are reachable services on those ports — so a screen learned on
 * one resolved (and its transition ran) on the other through the exact tier and
 * the title tie-break, which both key on this canonical form.
 */
export function canonicalUrl(raw: string | null | undefined): string {
  const trimmed = (raw ?? '').trim();
  if (!trimmed) return '';
  let url = trimmed;
  const hashIdx = url.indexOf('#');
  if (hashIdx !== -1) url = url.slice(0, hashIdx);
  try {
    const u = new URL(url);
    u.hash = '';
    if (
      (u.protocol === 'http:' && u.port === '80') ||
      (u.protocol === 'https:' && u.port === '443')
    ) {
      u.port = '';
    }
    u.hostname = u.hostname.toLowerCase();
    u.pathname = u.pathname.replace(/\/+$/, '') || '/';
    u.searchParams.sort();
    return u.toString();
  } catch {
    return trimmed.replace(/\s+/g, '').toLowerCase().replace(/\/+$/, '');
  }
}

/** Canonical URL equality; both sides must normalize to the same non-empty. */
export function canonicalUrlEqual(
  a: string | null | undefined,
  b: string | null | undefined
): boolean {
  const na = canonicalUrl(a);
  const nb = canonicalUrl(b);
  return na.length > 0 && na === nb;
}

// ---------------------------------------------------------------------------
// State-variant serialization.
// ---------------------------------------------------------------------------

/** Deterministic string form of a state-variant (for fingerprint + identity). */
export function serializeStateVariant(variant: StateVariantV01 | null | undefined): string {
  const modal = variant?.modal ?? '';
  const tab = variant?.tab ?? '';
  const frame = variant?.iframe ?? '';
  return `modal=${modal}|tab=${tab}|iframe=${frame}`;
}

/** Exact variant equality (absent/base vs absent/base are equal). */
export function stateVariantEqual(
  a: StateVariantV01 | null | undefined,
  b: StateVariantV01 | null | undefined
): boolean {
  return serializeStateVariant(a) === serializeStateVariant(b);
}

// ---------------------------------------------------------------------------
// Snapshot normalization + defensive bounding.
// ---------------------------------------------------------------------------

function boundSlice<T>(values: T[] | null | undefined, max: number): { items: T[]; truncated: boolean } {
  if (!Array.isArray(values)) return { items: [], truncated: false };
  if (values.length <= max) return { items: values, truncated: false };
  return { items: values.slice(0, max), truncated: true };
}

function boundString(value: string | null | undefined, max: number): string {
  if (typeof value !== 'string') return '';
  return value.length <= max ? value : value.slice(0, max);
}

/**
 * B-9: the title gets the SAME shape redaction R4F4 applies to headings.
 *
 * The write path already persists redacted titles (`BenchCRM — Organisation
 * <email>` — every promoted row proves it), but the live side used to carry
 * the RAW title into both identity comparisons: `liveFingerprint` could never
 * hash-match a `storedFingerprint` for any data-titled page (no tier-0), and
 * `titleDecisiveCandidate` compared byte-exact, so the one axis designed to
 * break list/detail twin ties was dark on exactly those pages. Result: every
 * post-promotion compiled run to a data-titled destination failed closed as
 * `screen unresolved` until the assertion budget expired.
 *
 * Safe by the same two properties as `redactHeadingText`:
 *  - DATA-INDEPENDENT: recognizes shapes (email, uuid, JWT, hex, digit runs),
 *    never a caller's values, so no value table is needed at resolve time.
 *  - IDEMPOTENT: placeholders cannot be re-matched, so re-redacting a stored
 *    (already-redacted) title is a no-op, and legacy raw rows become
 *    comparable instead of silently diverging.
 *
 * A real collision (two distinct screens whose titles differ ONLY in data
 * shapes, same signature) collapses to the C4 fingerprint-collision fail-closed
 * — the designed answer, identical to what heading redaction already does.
 */
function redactedTitle(value: string | null | undefined): string {
  if (typeof value !== 'string') return '';
  return boundString(redactHeadingText(value), RESOLVER_LIMITS.maxTitle);
}

export interface NormalizedSnapshot {
  canonicalUrl: string;
  title: string;
  headings: string[];
  landmarkRoles: string[];
  formFieldTypes: string[];
  interactiveRefs: ResolverInteractiveRef[];
  variant: StateVariantV01;
  authSignal: AuthModeV01 | null;
  truncated: boolean;
  byteSize: number;
}

/**
 * Bound + normalize a runtime snapshot into the resolver's canonical form.
 * Oversized inputs are deterministically truncated (drop beyond the cap) and
 * the fact is surfaced via `truncated`. Strings are trimmed of the surrogate
 * surface only (whitespace collapsed) — no content flattening.
 */
export function normalizeSnapshot(
  snapshot: ResolverSnapshotV01
): NormalizedSnapshot {
  let truncated = false;
  const mark = (b: boolean) => {
    if (b) truncated = true;
  };

  const headings = boundSlice(snapshot.headings, RESOLVER_LIMITS.maxHeadings);
  mark(headings.truncated);
  const landmarkRoles = boundSlice(
    snapshot.landmarkRoles,
    RESOLVER_LIMITS.maxLandmarks
  );
  mark(landmarkRoles.truncated);
  const forms = boundSlice(snapshot.forms, RESOLVER_LIMITS.maxForms);
  mark(forms.truncated);
  const interactiveRefs = boundSlice(
    snapshot.interactiveRefs,
    RESOLVER_LIMITS.maxRefs
  );
  mark(interactiveRefs.truncated);

  const tabList = boundSlice(
    snapshot.tabPanel?.tabs,
    RESOLVER_LIMITS.maxTabs
  );
  mark(tabList.truncated);

  const formFieldTypes: string[] = [];
  for (const form of forms.items) {
    const fields = boundSlice(
      form?.fieldTypes,
      RESOLVER_LIMITS.maxFieldsPerForm
    );
    mark(fields.truncated);
    for (const f of fields.items) {
      const bounded = boundString(f, RESOLVER_LIMITS.maxFieldLen);
      if (bounded) formFieldTypes.push(bounded);
    }
  }

  const normalized = buildSignature({
    headings: headings.items.map((h) => boundString(h, RESOLVER_LIMITS.maxHeadingLen)),
    landmarkRoles: landmarkRoles.items.map((l) => boundString(l, RESOLVER_LIMITS.maxLandmarkLen)),
    formFieldTypes,
  });

  const refs = interactiveRefs.items.map((r) => ({
    role: boundString(r?.role, RESOLVER_LIMITS.maxRefRoleLen),
    label: boundString(r?.label, RESOLVER_LIMITS.maxRefLabelLen),
  }));

  const variant: StateVariantV01 = {
    modal:
      snapshot.modal?.open === true
        ? boundString(snapshot.modal.kind ?? 'open', RESOLVER_LIMITS.maxTabLen)
        : undefined,
    tab:
      snapshot.tabPanel?.selected
        ? boundString(snapshot.tabPanel.selected, RESOLVER_LIMITS.maxTabLen)
        : undefined,
    iframe:
      snapshot.iframe?.present === true
        ? boundString(
            snapshot.iframe.identity ?? 'present',
            RESOLVER_LIMITS.maxIframeIdentityLen
          )
        : undefined,
  };

  const result: NormalizedSnapshot = {
    canonicalUrl: canonicalUrl(snapshot.url),
    title: redactedTitle(snapshot.title),
    headings: normalized.normalizedHeadings,
    landmarkRoles: normalized.landmarkRoles,
    formFieldTypes: normalized.formFieldTypes,
    interactiveRefs: refs,
    variant,
    authSignal: snapshot.authSignal ?? null,
    truncated,
    byteSize: 0,
  };

  // Structural byte size of the bounded snapshot (no titles/refs labels in the
  // byte count are excluded — only bounded structural lists) for telemetry.
  result.byteSize = JSON.stringify({
    u: result.canonicalUrl,
    t: result.title,
    h: result.headings,
    l: result.landmarkRoles,
    f: result.formFieldTypes,
    r: refs.length,
    v: serializeStateVariant(variant),
  }).length;

  return result;
}

// ---------------------------------------------------------------------------
// Evidence scoring.
// ---------------------------------------------------------------------------

function liveFingerprint(norm: NormalizedSnapshot): string {
  return computeScreenFingerprint({
    url: norm.canonicalUrl,
    title: norm.title,
    stateVariant: serializeStateVariant(norm.variant),
    authMode: norm.authSignal,
    signature: {
      landmarkRoles: norm.landmarkRoles,
      formFieldTypes: norm.formFieldTypes,
      normalizedHeadings: norm.headings,
    },
  });
}

function storedFingerprint(candidate: StoredScreenV01): string {
  return computeScreenFingerprint({
    url: canonicalUrl(candidate.url),
    title: candidate.title,
    stateVariant: serializeStateVariant(candidate.stateVariant),
    authMode: candidate.storedAuthMode,
    signature: storedSignature(candidate),
  });
}

/**
 * Coerce an unknown value to a BOUNDED string set (fail-safe on malformed
 * input). Unlike the live-snapshot path, stored signatures are re-bounded here
 * too: element count capped at `maxCount` and each string length-capped at
 * `maxLen`, so a pathological `semantic_signature` in the DB can never inflate
 * a fingerprint or a Jaccard score beyond the same bounds the live snapshot
 * respects. Sorted + deduped by `buildSignature` downstream.
 */
function safeSet(value: unknown, maxCount: number, maxLen: number): string[] {
  if (!Array.isArray(value)) return [];
  const out: string[] = [];
  for (const v of value) {
    if (typeof v !== 'string') continue;
    const bounded = v.length <= maxLen ? v : v.slice(0, maxLen);
    if (bounded) out.push(bounded);
    if (out.length >= maxCount) break;
  }
  return out;
}

function storedSignature(candidate: StoredScreenV01): SemanticSignature {
  const s = candidate.signature as Partial<SemanticSignature> | null | undefined;
  if (!s) return { landmarkRoles: [], formFieldTypes: [], normalizedHeadings: [] };
  return {
    landmarkRoles: safeSet(s.landmarkRoles, RESOLVER_LIMITS.maxLandmarks, RESOLVER_LIMITS.maxLandmarkLen),
    formFieldTypes: safeSet(
      s.formFieldTypes,
      RESOLVER_LIMITS.maxForms * RESOLVER_LIMITS.maxFieldsPerForm,
      RESOLVER_LIMITS.maxFieldLen
    ),
    // R4F4 (round 4): stored headings are redacted with the SAME idempotent,
    // data-independent rules `buildSignature` applies to the live side. Rows
    // written before redaction existed still hold raw values, so redacting only
    // at write time would leave the live (redacted) and legacy stored (raw)
    // headings unable to match — and the fingerprint, which is recomputed for
    // both sides at resolve time, asymmetric. Redacting here makes new rows,
    // legacy rows and the live page all canonicalize identically.
    normalizedHeadings: safeSet(s.normalizedHeadings, RESOLVER_LIMITS.maxHeadings, RESOLVER_LIMITS.maxHeadingLen).map(
      (heading) => redactHeadingText(heading)
    ),
  };
}

/** Interactive-structure (live-ref) similarity vs the stored affordance surface. */
function liveRefScore(
  norm: NormalizedSnapshot,
  stored: SemanticSignature
): number {
  const liveAffordances = new Set<string>();
  for (const r of norm.interactiveRefs) {
    const role = r.role.trim().toLowerCase();
    if (role) liveAffordances.add(role);
  }
  for (const l of norm.landmarkRoles) liveAffordances.add(l);
  const storedAffordances = new Set<string>([
    ...stored.landmarkRoles,
    ...stored.formFieldTypes,
  ]);
  return fieldJaccard([...liveAffordances], [...storedAffordances]);
}

function evidenceTier(args: {
  fingerprintExact: boolean;
  urlExact: boolean;
  routeMatch: boolean;
  variantMatch: boolean;
  semantic: number;
  liveRef: number;
  authConflict: boolean;
  /** True when the LIVE snapshot carries at least one interactive ref. */
  hasLiveRefs: boolean;
}): EvidenceTier | null {
  if (args.fingerprintExact) return 0;
  if (
    args.urlExact &&
    args.routeMatch &&
    args.variantMatch &&
    args.semantic >= STRUCTURE_CONFLICT_FLOOR &&
    !args.authConflict
  ) {
    return 1;
  }
  if (
    (!LIVE_REF_REQUIRES_REFS || args.hasLiveRefs) &&
    args.liveRef >= LIVE_REF_STRONG &&
    args.semantic >= STRUCTURE_CONFLICT_FLOOR
  ) {
    return 2;
  }
  return 3;
}

function tierConfidence(tier: EvidenceTier, semantic: number, liveRef: number): number {
  switch (tier) {
    case 0:
      return 0.95;
    case 1:
      return Math.min(0.95, 0.75 + 0.2 * semantic);
    case 2:
      return Math.min(0.9, 0.55 + 0.35 * liveRef);
    default:
      return semantic;
  }
}

function tierMatchedBy(tier: EvidenceTier): ScreenIdentityV01['matchedBy'] {
  switch (tier) {
    case 0:
      return ['fingerprint'];
    case 1:
      return ['route_family', 'semantic_signature'];
    case 2:
      return ['live_ref', 'semantic_signature'];
    default:
      return ['semantic_signature'];
  }
}

function scoreCandidate(
  candidate: StoredScreenV01,
  norm: NormalizedSnapshot,
  liveRoute: string | null | undefined,
  currentCacheVersion: string | null | undefined,
  weights: SignatureWeights,
  liveFp: string,
  liveOriginKey: string | null
): ScoredCandidate {
  const signature = storedSignature(candidate);
  const cUrl = canonicalUrl(candidate.url);
  const urlExact = cUrl.length > 0 && cUrl === norm.canonicalUrl;
  const routeMatch = routeFamilyMatch(candidate.routeFamily, liveRoute);
  const variantMatch = stateVariantEqual(candidate.stateVariant, norm.variant);
  const semantic = weightedJaccard(
    {
      landmarkRoles: norm.landmarkRoles,
      formFieldTypes: norm.formFieldTypes,
      normalizedHeadings: norm.headings,
    },
    signature,
    weights
  );
  const liveRef = liveRefScore(norm, signature);
  const fingerprint = storedFingerprint(candidate);
  const fingerprintExact = liveFp === fingerprint;
  const authConflict =
    !!norm.authSignal &&
    !!candidate.storedAuthMode &&
    norm.authSignal !== candidate.storedAuthMode;
  const structureConflict =
    urlExact && semantic < STRUCTURE_CONFLICT_FLOOR;
  const candOriginKey = originKey(candidate.url);
  const originConflict =
    liveOriginKey !== null &&
    candOriginKey !== null &&
    candOriginKey !== liveOriginKey;
  // R7: the live origin is known and this row cannot prove one. Fail closed —
  // see `originUnprovable` on RankedResolution.
  const originUnprovable = liveOriginKey !== null && candOriginKey === null;
  // H3 fail-closed: when a cache version is expected (currentCacheVersion set),
  // a row is FRESH only if it carries a version EQUAL to current. A missing
  // version is not "fresh" — it is stale/unversioned and excluded from eligible.
  const cacheUnversioned =
    !!currentCacheVersion && !candidate.cacheVersion;
  const cacheStale =
    !!currentCacheVersion &&
    candidate.cacheVersion !== currentCacheVersion;

  const tier = evidenceTier({
    fingerprintExact,
    urlExact,
    routeMatch,
    variantMatch,
    semantic,
    liveRef,
    authConflict,
    hasLiveRefs: norm.interactiveRefs.length > 0,
  });

  return {
    screenId: candidate.screenId,
    stateKey: candidate.stateKey,
    url: candidate.url,
    routeFamily: candidate.routeFamily ?? null,
    stateVariant: serializeStateVariant(candidate.stateVariant ?? norm.variant),
    confidence: tier === null ? 0 : tierConfidence(tier, semantic, liveRef),
    matchedBy: tier === null ? ['semantic_signature'] : tierMatchedBy(tier),
    tier: tier === null ? 3 : tier,
    semanticScore: semantic,
    liveRefScore: liveRef,
    fingerprintExact,
    authConflict,
    structureConflict,
    originConflict,
    originUnprovable,
    cacheStale,
    cacheUnversioned,
    fingerprint,
    title: redactedTitle(candidate.title),
    storedAuthMode: candidate.storedAuthMode ?? null,
    cacheVersion: candidate.cacheVersion ?? null,
  };
}

/** Stable tie-break independent of DB row order: (stateKey, url, screenId). */
function compareRanked(a: RankedResolution, b: RankedResolution): number {
  if (a.tier !== b.tier) return a.tier - b.tier;
  if (b.confidence !== a.confidence) return b.confidence - a.confidence;
  if (a.stateKey !== b.stateKey) return a.stateKey < b.stateKey ? -1 : 1;
  if (a.url !== b.url) return a.url < b.url ? -1 : 1;
  return a.screenId - b.screenId;
}

/**
 * R6F3 (round 6, finding 3): ORIGIN + PATHNAME identity key — the part of a url
 * a title is NEVER allowed to bridge.
 *
 * Built from `canonicalUrl` so it agrees with the exact-fingerprint tier on host
 * case, default ports and trailing slashes, then drops ONLY the query. The query
 * is the single axis the store cannot keep (it may carry secrets) and therefore
 * the only axis the title tie-break may bypass; scheme, host, port and pathname
 * must match exactly.
 *
 * Returns null for anything that cannot PROVE an origin — an empty url, a bare
 * path (`/app`), a non-http(s) scheme, junk — so an unprovable url fails closed
 * instead of being treated as "same origin".
 */
function originAndPathKey(raw: string | null | undefined): string | null {
  const canonical = canonicalUrl(raw);
  if (!canonical) return null;
  try {
    const u = new URL(canonical);
    if (u.protocol !== 'http:' && u.protocol !== 'https:') return null;
    return `${u.origin}${u.pathname.replace(/\/+$/, '') || '/'}`;
  } catch {
    return null;
  }
}

/**
 * B-10: origin + pathname key with NUMERIC record segments generalized — the
 * tie-break gate's notion of "the same page".
 *
 * `originAndPathKey` requires a byte-equal pathname, which a parameterized
 * route can never satisfy across two records: a flow that CREATES an entity
 * lands on a freshly minted id (`/clients/client/13849` stored vs
 * `/clients/client/13850` live), so every stored sibling row fails the gate,
 * the band cannot elect on the (B-9) redacted title, and the pool fails closed
 * as `screen_ambiguous` → `screen unresolved` until the assertion budget
 * expires (measured live: crm.anhtester.com detail destination, 45.7 s).
 *
 * Generalization is exactly the rule the destination gate already uses in
 * `flow-executor.routeTemplate`: a purely-numeric segment that FOLLOWS other
 * segments is a record id and folds to `*`; a bare numeric root (`/42` vs
 * `/77`) is an identifier, not a record under a route, and stays distinct.
 * Everything else — scheme, host, port, segment COUNT, and every non-numeric
 * segment (so tabbed shapes like `/client/9/attachments` stay separate pages,
 * and the R6F3 pool where twins sit on DIFFERENT non-numeric paths cannot be
 * bridged) — must still match byte-exactly. Only the query axis (R6F2) and the
 * numeric record-id axis (B-10) may be bypassed, and only WITHIN one origin.
 *
 * Returns null whenever the url cannot prove an origin — the unprovable shapes
 * fail closed exactly as before.
 */
function originAndRecordPathKey(raw: string | null | undefined): string | null {
  const strict = originAndPathKey(raw);
  if (strict === null) return null;
  try {
    const u = new URL(strict);
    const segs = u.pathname.split('/');
    const meaningful = segs.filter((s) => s.length > 0).length;
    if (meaningful < 2) return strict;
    const norm = segs.map((s, idx) => (idx > 0 && /^\d+$/.test(s) ? '*' : s));
    return `${u.origin}${norm.join('/')}`;
  } catch {
    return strict;
  }
}

/**
 * Origin-only identity key — `scheme://host[:port]` exactly as WHATWG
 * canonicalizes it (only the scheme-default port is dropped, host lowercased).
 * The eligibility gate compares THIS (not the pathname): the semantic and route
 * tiers are allowed to cross pathnames on one origin, but no tier may cross an
 * origin boundary. Returns null for anything that cannot PROVE an origin — an
 * empty url, a bare path, a non-http(s) scheme, junk — so legacy path-only rows
 * and unprovable live urls keep their pre-gate behavior instead of being
 * silently treated as "same origin" or "different origin".
 */
function originKey(raw: string | null | undefined): string | null {
  try {
    const u = new URL((raw ?? '').trim());
    if (u.protocol !== 'http:' && u.protocol !== 'https:') return null;
    return u.origin.toLowerCase();
  } catch {
    return null;
  }
}

/**
 * R6F2 (round 6, finding 2): TITLE TIE-BREAK inside the undifferentiated band.
 *
 * `title` is a fingerprint axis (tier 0) and nothing else, so two screens
 * rendered from ONE template — same canonical url, same route family, same
 * semantic signature, different `<title>` — stay separable only while the exact
 * fingerprint tier can fire. That tier compares FULL canonical urls, so it goes
 * dark as soon as the stored url and the live url disagree on anything
 * `canonicalUrl` preserves: scheme and port (`https://h/app` vs
 * `http://h:8797/app`) or query (`/app` vs `/app?record=13518`). Below tier 0
 * the twins are identical by construction, tie inside `ambiguityBand`, and the
 * pool fails closed as `screen_ambiguous` — measured on both shapes above.
 *
 * Storing the live query is not an option (it can carry secrets), so the tie is
 * broken on the axis that already separates the rows. This only ever converts an
 * ambiguous verdict into a match, never one match into another, and every
 * condition below must hold or the band is left exactly as it was:
 *
 *   - the live url and EVERY tied row's url share an origin AND the same page
 *     (R6F3). A title must never carry a stored row across a scheme, host, port
 *     or path boundary: two services on one hostname but different ports are
 *     different services, and a draft learned on one of them would otherwise
 *     replay — and mutate — on the other. The page identity is the RECORD-AWARE
 *     pathname key (B-10 `originAndRecordPathKey`): the query may be bypassed,
 *     and a numeric record-id segment may differ across records of ONE
 *     parameterized route (a create-flow lands on a freshly minted id —
 *     `/clients/client/13849` stored vs `/…/13850` live — and must stay the
 *     same page), but segment count and every non-numeric segment still match
 *     byte-exactly. A url that cannot prove an origin fails closed;
 *   - the live snapshot carries a title;
 *   - EVERY tied row carries a non-empty title — a null-titled row may BE this
 *     screen under a title nobody recorded, so matching around it is a guess;
 *   - the tied titles are pairwise distinct, i.e. the axis really discriminates;
 *   - exactly ONE tied title equals the live one, byte-exact and
 *     case-sensitive — the same comparison the fingerprint axis performs.
 *     Both sides are SHAPE-redacted first (B-9 `redactedTitle`), so a stored
 *     `Organisation <email>` row is comparable with a live
 *     `Organisation jane@corp.com` title while the comparison itself stays
 *     byte-exact and case-sensitive;
 *   - the elected row clears `minConfidence` on its own, since it may sit up to
 *     one band below the row that already passed that gate.
 */
function titleDecisiveCandidate(
  tied: readonly ScoredCandidate[],
  liveTitle: string,
  liveUrl: string,
  minConfidence: number
): ScoredCandidate | null {
  if (!liveTitle || tied.length < 2) return null;
  // R6F3: the origin+page gate comes FIRST — it is the security precondition,
  // and every tied row must clear it. Requiring all of them (not just the elected
  // one) means a pool that mixes origins can never elect the foreign row.
  // B-10: the path axis is record-aware (numeric record ids fold), so sibling
  // records of one parameterized route are the SAME page for this gate; the
  // origin axis stays strict.
  const liveKey = originAndRecordPathKey(liveUrl);
  if (!liveKey) return null;
  if (!tied.every((c) => originAndRecordPathKey(c.url) === liveKey)) return null;
  const titles = tied.map((c) => c.title);
  if (titles.some((t) => t.length === 0)) return null;
  if (new Set(titles).size !== titles.length) return null;
  let elected: ScoredCandidate | null = null;
  for (let i = 0; i < tied.length; i += 1) {
    if (titles[i] !== liveTitle) continue;
    if (elected) return null;
    elected = tied[i];
  }
  if (!elected) return null;
  return elected.confidence >= minConfidence ? elected : null;
}

/**
 * B-12 v2 (2026-09-03, live CRM cold c3 + review): ROUTE-SHAPE TIE-BREAK inside
 * the undifferentiated band — the URL axis that separates screens which the CRM
 * renders with one semantic signature.
 *
 * Measured shape: crm.anhtester.com renders record pages with divs, not h-tags,
 * so the clients LIST, the client ADD form and a client DETAIL share one
 * semantic signature (landmarks [navigation], forms [input:search], headings
 * []). A create-flow lands on a freshly minted detail (`/clients/client/13851`,
 * title = the new company): tier 0 is dark (url differs by record id AND title
 * is unseen data), so the band ties LIST/ADD/DETAIL rows at confidence 1.000.
 * This is NOT the R6F2 twins case — the rows are genuinely DIFFERENT screens,
 * and the live URL itself states which one: `/clients/client` is the ADD page,
 * `/clients/client/<id>` is a DETAIL page. The axis is the B-10 RECORD-AWARE
 * FOLD (`originAndRecordPathKey`) — origin + pathname with numeric record ids
 * folded — so a live detail url claims the detail TEMPLATE, never the add
 * screen's. (v1 matched stored `route_family` STRINGS; the review proved that
 * turns live detail into `state-add` — same template family, wrong functional
 * screen — and family labels are noisy in this pool anyway
 * (`admin/clients/:detail` vs `admin/clients/client` for one route). The fold
 * is derived from the row's own url, so no label can mislead it.)
 *
 * Every condition must hold or the band is left exactly as it was:
 *   - the live url proves origin+page (fold non-null);
 *   - EVERY tied row proves a page — a fold-less row may BE this screen under a
 *     url nobody recorded, which makes the election a guess;
 *   - EXACTLY ONE tied row's fold equals the live fold. Rows on other shapes
 *     (list, add, a foreign origin — the fold embeds origin, so exact-origin
 *     identity is REQUIRED to claim) are precisely the ones this evidence
 *     excludes; rows on the SAME shape as live are indistinguishable by url,
 *     and more than one of them means the band stays ambiguous unless the title
 *     axis (which runs first) decided it;
 *   - the live title does not POSITIVELY name a different tied row. The shape
 *     axis speaks only when the title is silent (entity data no row has ever
 *     seen). If some OTHER tied row carries exactly the live title, the title
 *     is evidence pointing elsewhere — typically R6F2 query-twins sharing one
 *     path (`/app?tab=list` vs `/app?tab=detail`, which the fold cannot see) —
 *     and overriding a byte-exact title match with coarse url evidence is a
 *     guess in the opposite direction. When the title names the SAME row shape
 *     elected, the two axes reinforce and election stands;
 *   - the elected row clears `minConfidence` on its own.
 */
function routeShapeDecisiveCandidate(
  tied: readonly ScoredCandidate[],
  liveUrl: string,
  liveTitle: string,
  minConfidence: number
): ScoredCandidate | null {
  if (tied.length < 2) return null;
  const liveKey = originAndRecordPathKey(liveUrl);
  if (!liveKey) return null;
  let elected: ScoredCandidate | null = null;
  for (const c of tied) {
    const key = originAndRecordPathKey(c.url);
    if (key === null) return null; // row proves no page identity — election would be a guess
    if (key === liveKey) {
      if (elected) return null; // >1 row claims THIS page/template with one signature
      elected = c;
    }
  }
  if (!elected) return null; // no band member is the page the live url names
  if (liveTitle) {
    for (const c of tied) {
      // Both sides arrive SHAPE-redacted (B-9): `norm.title` and ScoredCandidate
      // titles run through `redactedTitle` before reaching here, so this byte
      // comparison means the same thing as `titleDecisiveCandidate`'s.
      if (c !== elected && c.title === liveTitle) return null; // title names another tied row — fail closed
    }
  }
  return elected.confidence >= minConfidence ? elected : null;
}

// ---------------------------------------------------------------------------
// Resolution.
// ---------------------------------------------------------------------------

function buildIdentity(
  winner: RankedResolution,
  norm: NormalizedSnapshot,
  authMode: AuthModeV01,
  driftReason: ScreenIdentityV01['driftReason'],
  cacheVersion: string | null | undefined
): ScreenIdentityV01 {
  return {
    screenId: winner.screenId,
    stateKey: winner.stateKey,
    stateVariant: serializeStateVariant(norm.variant),
    url: winner.url,
    routeFamily: winner.routeFamily ?? null,
    confidence: winner.confidence,
    matchedBy: winner.matchedBy,
    // The ACTUAL cache version of the resolved stored row — never the live
    // current version. Stale rows are rejected before this point, so a resolved
    // screen's version is always the row's own (== current when not stale).
    cacheVersion: cacheVersion ?? undefined,
    authMode,
    driftReason,
  };
}

// ---------------------------------------------------------------------------
// Bounded cheap-evidence shortlist (H1).
// ---------------------------------------------------------------------------

/**
 * Deterministic cheap pre-rank key for a candidate row, computed WITHOUT any
 * full semantic scan. Used ONLY to select WHICH rows the shortlist keeps when a
 * priority tier exceeds the budget, and to pre-filter the fallback pool:
 *   - variantOk     same-URL SPA variant compatibility (0/1)
 *   - authOk        C1 auth-signal compatibility (0/1)
 *   - fresh         stored cache version === current (0/1) — version freshness
 *   - shared        cheap token-overlap count between the stored signature and
 *                   the live signature sets — a monotone upper-bound proxy for
 *                   weighted Jaccard (Jaccard = shared / union <= shared / |live|)
 *   - screenId      numeric screen_states.id as the final tie-break
 * The key is independent of DB row order and identical inputs always rank
 * identically.
 */
interface CheapRankKey {
  variantOk: 0 | 1;
  authOk: 0 | 1;
  fresh: 0 | 1;
  shared: number;
  screenId: number;
}

function sharedTokenCount(
  c: StoredScreenV01,
  liveLandmarks: ReadonlySet<string>,
  liveFields: ReadonlySet<string>,
  liveHeadings: ReadonlySet<string>
): number {
  const sig = storedSignature(c);
  let shared = 0;
  for (const s of sig.landmarkRoles) if (liveLandmarks.has(s)) shared += 1;
  for (const s of sig.formFieldTypes) if (liveFields.has(s)) shared += 1;
  for (const s of sig.normalizedHeadings) if (liveHeadings.has(s)) shared += 1;
  return shared;
}

function cheapRankKey(
  c: StoredScreenV01,
  norm: NormalizedSnapshot,
  currentCacheVersion: string | null | undefined,
  variant: boolean,
  liveLandmarks: ReadonlySet<string>,
  liveFields: ReadonlySet<string>,
  liveHeadings: ReadonlySet<string>
): CheapRankKey {
  return {
    variantOk: variant && stateVariantEqual(c.stateVariant, norm.variant) ? 1 : 0,
    authOk:
      !(!!norm.authSignal && !!c.storedAuthMode && norm.authSignal !== c.storedAuthMode)
        ? 1
        : 0,
    fresh:
      !!currentCacheVersion && c.cacheVersion === currentCacheVersion ? 1 : 0,
    shared: sharedTokenCount(c, liveLandmarks, liveFields, liveHeadings),
    screenId: c.screenId,
  };
}

function compareCheapRank(a: CheapRankKey, b: CheapRankKey): number {
  return (
    b.variantOk - a.variantOk ||
    b.authOk - a.authOk ||
    b.fresh - a.fresh ||
    b.shared - a.shared ||
    a.screenId - b.screenId
  );
}

interface ShortlistBuild {
  shortlist: StoredScreenV01[];
  byUrl: number;
  byRoute: number;
  byFallback: number;
  /** Pool counts BEFORE the budget — saturation is `total > by*`. */
  urlExactTotal: number;
  routeTotal: number;
  /**
   * Fingerprints of URL-exact rows DROPPED by a saturated budget. Populated only
   * when the URL-exact tier exceeds the budget; used to detect a C4 collision
   * split across the keep/drop cutoff (two distinct numeric IDs with the same
   * exact fingerprint — fail closed).
   */
  droppedUrlFingerprints: string[];
  /** Auth-conflicting rows kept by the fallback (informational — never excluded). */
  authConflictIncluded: number;
}

/**
 * Deterministic, HARD-bounded candidate shortlist built BEFORE scoring (H1).
 *
 * Cheap-evidence tiers, in order:
 *   1. canonical-URL-exact rows — kept first, up to the budget.
 *   2. route-family matches (exact family only when both sides non-empty).
 *   3. deterministic fallback: the rest pool cheap-pre-ranked by the token-
 *      overlap key, full-scored up to `fallbackBudget + prefilterSlack`, then
 *      trimmed to the fallback budget by exact weighted Jaccard.
 *
 * HARD INVARIANT: `shortlist.length <= budget` for EVERY input. A priority tier
 * with more rows than the budget can hold is saturated: the extra rows are
 * reported (urlExactTotal / routeTotal + droppedUrlFingerprints) so the caller
 * fails closed instead of silently resolving on dropped evidence.
 *
 * The result depends only on `list` contents, not DB row order.
 */
function buildShortlist(
  list: readonly StoredScreenV01[],
  norm: NormalizedSnapshot,
  liveRoute: string | null | undefined,
  budget: number,
  currentCacheVersion: string | null | undefined,
  weights: SignatureWeights
): ShortlistBuild {
  const urlExact: StoredScreenV01[] = [];
  const routeMatch: StoredScreenV01[] = [];
  const rest: StoredScreenV01[] = [];
  const normUrl = norm.canonicalUrl;

  for (const c of list) {
    if (canonicalUrl(c.url) === normUrl) urlExact.push(c);
    else if (routeFamilyMatch(c.routeFamily, liveRoute)) routeMatch.push(c);
    else rest.push(c);
  }

  const liveLandmarks = new Set(norm.landmarkRoles);
  const liveFields = new Set(norm.formFieldTypes);
  const liveHeadings = new Set(norm.headings);
  const keyFor = (c: StoredScreenV01, variant: boolean) =>
    cheapRankKey(
      c,
      norm,
      currentCacheVersion,
      variant,
      liveLandmarks,
      liveFields,
      liveHeadings
    );

  // Deterministic pre-rank for each tier. URL-exact rows rank by (variant, auth,
  // freshness, shared tokens, screenId); route-family rows by the same key. The
  // whole POOL is cheap-pre-ranked (token overlap only, no weighted Jaccard);
  // only the budgeted rows are later full-scored — bounding construction work
  // by pool size.
  const urlExactRanked = urlExact
    .map((c) => ({ c, key: keyFor(c, true) }))
    .sort((a, b) => compareCheapRank(a.key, b.key));
  const routeRanked = routeMatch
    .map((c) => ({ c, key: keyFor(c, true) }))
    .sort((a, b) => compareCheapRank(a.key, b.key));

  // ---- Hard bound: shortlist.length <= budget for EVERY input. ----
  const shortlist: StoredScreenV01[] = [];
  let keptUrl = 0;
  let keptRoute = 0;
  let byFallback = 0;
  let authConflictIncluded = 0;

  for (const r of urlExactRanked) {
    if (keptUrl >= budget) break;
    shortlist.push(r.c);
    keptUrl += 1;
  }
  const droppedUrl = urlExactRanked.slice(keptUrl);
  const droppedUrlFingerprints =
    droppedUrl.length > 0
      ? droppedUrl.map((r) => storedFingerprint(r.c))
      : [];

  const routeBudget = Math.max(0, budget - keptUrl);
  for (const r of routeRanked) {
    if (keptRoute >= routeBudget) break;
    shortlist.push(r.c);
    keptRoute += 1;
  }

  const fallbackBudget = Math.max(0, budget - keptUrl - keptRoute);
  if (fallbackBudget > 0 && rest.length > 0) {
    // Cheap prefilter over the WHOLE rest pool (token overlap only, no semantic
    // scan), then exact weighted-Jaccard on the prefiltered top (bounded by
    // fallbackBudget + prefilterSlack), then a final deterministic trim to the
    // fallback budget.
    const liveSig: SemanticSignature = {
      landmarkRoles: norm.landmarkRoles,
      formFieldTypes: norm.formFieldTypes,
      normalizedHeadings: norm.headings,
    };
    const prefiltered = rest
      .map((c) => ({ c, key: keyFor(c, false) }))
      .sort((a, b) => compareCheapRank(a.key, b.key))
      .slice(0, fallbackBudget + RESOLVER_LIMITS.prefilterSlack);
    const exactRanked = prefiltered
      .map((r) => ({
        c: r.c,
        authCompatible: r.key.authOk,
        sem: weightedJaccard(liveSig, storedSignature(r.c), weights),
      }))
      .sort(
        (a, b) =>
          b.authCompatible - a.authCompatible ||
          b.sem - a.sem ||
          a.c.screenId - b.c.screenId
      );
    for (const r of exactRanked) {
      if (byFallback >= fallbackBudget) break;
      shortlist.push(r.c);
      byFallback += 1;
      if (!r.authCompatible) authConflictIncluded += 1;
    }
  }

  return {
    shortlist,
    byUrl: keptUrl,
    byRoute: keptRoute,
    byFallback,
    urlExactTotal: urlExact.length,
    routeTotal: routeMatch.length,
    droppedUrlFingerprints,
    authConflictIncluded,
  };
}

/**
 * Resolve the current screen from a bounded snapshot against stored
 * candidates. Pure and deterministic — identical (bounded) inputs yield
 * byte-identical results; never DB-row-order dependent.
 */
export function resolveScreen(
  snapshot: ResolverSnapshotV01,
  candidates: readonly StoredScreenV01[],
  options: ResolveScreenOptionsV01 = {}
): ScreenResolutionV01 {
  const started = performance.now();
  const minConfidence = options.minConfidence ?? DEFAULT_MIN_CONFIDENCE;
  const ambiguityBand = options.ambiguityBand ?? DEFAULT_AMBIGUITY_BAND;
  const weights = options.weights ?? DEFAULT_SIGNATURE_WEIGHTS;
  // H1: bounded by default. `maxCandidates` bounds the cheap-evidence shortlist
  // built BEFORE scoring; the shortlist is HARD-bounded (`shortlist.length <=
  // normalizedBudget` for every input). Invalid budgets are normalized first.
  const maxCandidates = normalizeBudget(
    options.maxCandidates,
    RESOLVER_LIMITS.maxCandidates
  );
  const maxMatches = options.maxMatches ?? RESOLVER_LIMITS.maxMatches;
  const rules: string[] = [];

  const norm = normalizeSnapshot(snapshot);
  const list = Array.isArray(candidates) ? candidates : [];
  if (!norm.canonicalUrl) {
    return {
      status: 'unknown',
      reason: 'screen_unknown',
      screen: null,
      matches: [],
      telemetry: {
        inputTruncated: norm.truncated,
        inputByteSize: norm.byteSize,
        candidateCount: 0,
        poolTotal: list.length,
        shortlistBudget: maxCandidates,
        shortlistKept: 0,
        shortlistDropped: list.length,
        shortlistByUrl: 0,
        shortlistByRoute: 0,
        shortlistByFallback: 0,
        shortlistSaturatedTier: null,
        rules: ['no_url'],
        durationMs: Math.max(0, performance.now() - started),
      },
    };
  }

  // ---- H1 bounded cheap-evidence shortlist (BEFORE scoring) ----
  // URL-exact rows are kept first (up to the budget), then route-family
  // matches, then a deterministic fallback allocation. Only the shortlist is
  // scored. A priority tier that exceeds the budget is SATURATED and reported
  // (shortlistSaturatedTier + shortlist_saturated_url/_route); truncation is
  // surfaced via `shortlistDropped` and the `shortlist_truncated` rule.
  const shortlistInfo = buildShortlist(
    list,
    norm,
    options.routeFamily,
    maxCandidates,
    options.currentCacheVersion,
    weights
  );
  const poolTotal = list.length;
  const shortlistKept = shortlistInfo.shortlist.length;
  const shortlistDropped = poolTotal - shortlistKept;
  const saturatedUrl = shortlistInfo.urlExactTotal > shortlistInfo.byUrl;
  const saturatedRoute = shortlistInfo.routeTotal > shortlistInfo.byRoute;
  const shortlistSaturatedTier: ScreenResolutionV01['telemetry']['shortlistSaturatedTier'] =
    saturatedUrl && saturatedRoute
      ? 'url,route'
      : saturatedUrl
        ? 'url'
        : saturatedRoute
          ? 'route'
          : null;
  if (shortlistDropped > 0) rules.push('shortlist_truncated');

  // The live fingerprint depends only on the snapshot — compute once.
  const liveFp = liveFingerprint(norm);
  // Origin-eligibility gate: computed once, compared per candidate.
  const liveOriginKey = originKey(norm.canonicalUrl);
  const scored = shortlistInfo.shortlist.map((c) =>
    scoreCandidate(
      c,
      norm,
      options.routeFamily,
      options.currentCacheVersion,
      weights,
      liveFp,
      liveOriginKey
    )
  );

  const baseTelemetry = {
    inputTruncated: norm.truncated,
    inputByteSize: norm.byteSize,
    candidateCount: scored.length,
    poolTotal,
    shortlistBudget: maxCandidates,
    shortlistKept,
    shortlistDropped,
    shortlistByUrl: shortlistInfo.byUrl,
    shortlistByRoute: shortlistInfo.byRoute,
    shortlistByFallback: shortlistInfo.byFallback,
    shortlistSaturatedTier,
  };

  if (scored.length === 0) {
    return {
      status: 'unknown',
      reason: 'screen_unknown',
      screen: null,
      matches: [],
      telemetry: {
        ...baseTelemetry,
        rules: ['no_candidates'],
        durationMs: Math.max(0, performance.now() - started),
      },
    };
  }

  const sorted = [...scored].sort(compareRanked);
  // Stale fingerprint evidence is REJECTED (P2 contract: "stale fingerprint
  // drift rejection"). A row stored under an older cacheVersion is never
  // eligible for resolution; `staleAny` lets a non-resolving pool report
  // screen_stale (the screen is known, its stored fingerprint is stale) rather
  // than a bare ambiguous.
  const staleAny = scored.some((s) => s.cacheStale);
  const eligible = sorted.filter(
    (s) =>
      !s.authConflict &&
      !s.structureConflict &&
      !s.cacheStale &&
      !s.originConflict &&
      !s.originUnprovable
  );

  // ---- Structure drift: the only URL match fundamentally disagrees ----
  const structureDrifted = scored.some((s) => s.structureConflict);
  if (eligible.length === 0) {
    if (structureDrifted) {
      return {
        status: 'stale',
        reason: 'screen_stale',
        screen: null,
        matches: topMatches(sorted, maxMatches),
        telemetry: {
          ...baseTelemetry,
          rules: ['structure_conflict', 'screen_stale'],
          durationMs: Math.max(0, performance.now() - started),
        },
      };
    }
    if (staleAny) {
      // H3: distinguish "stored under an older version" from "version missing
      // entirely" (missing is NOT fresh — fail closed).
      const unversioned = scored.some((s) => s.cacheUnversioned);
      return {
        status: 'stale',
        reason: 'screen_stale',
        screen: null,
        matches: topMatches(sorted, maxMatches),
        telemetry: {
          ...baseTelemetry,
          rules: unversioned
            ? ['missing_cache_version', 'screen_stale']
            : ['stale_fingerprint', 'screen_stale'],
          durationMs: Math.max(0, performance.now() - started),
        },
      };
    }
    // Origin gate: nothing survived and the exclusions were origin-based (no
    // structural drift, no staleness claimed the pool first) — either a PROVEN
    // different origin (`origin_conflict`) or a row that cannot prove its origin
    // at all while the live url can (`origin_unprovable`, R7). For THIS origin
    // the screen is simply not known — report screen_unknown so the learning
    // loop treats it as a new screen instead of an ambiguity of rows that may
    // belong to a different service.
    if (scored.some((s) => s.originConflict || s.originUnprovable)) {
      return {
        status: 'unknown',
        reason: 'screen_unknown',
        screen: null,
        matches: [],
        telemetry: {
          ...baseTelemetry,
          rules: [
            ...(scored.some((s) => s.originConflict) ? ['origin_conflict'] : []),
            ...(scored.some((s) => s.originUnprovable) ? ['origin_unprovable'] : []),
          ],
          durationMs: Math.max(0, performance.now() - started),
        },
      };
    }
    return {
      status: 'ambiguous',
      reason: 'screen_ambiguous',
      screen: null,
      matches: topMatches(sorted, maxMatches),
      telemetry: {
        ...baseTelemetry,
        rules: ['auth_conflict'],
        durationMs: Math.max(0, performance.now() - started),
      },
    };
  }

  // R6F2: reassigned when a title tie-break elects a different tied row.
  let best = eligible[0];

  // ---- Low confidence fails closed ----
  if (best.confidence < minConfidence) {
    return {
      status: 'ambiguous',
      reason: 'screen_ambiguous',
      screen: null,
      matches: topMatches(sorted, maxMatches),
      telemetry: {
        ...baseTelemetry,
        rules: ['low_confidence', `best=${best.confidence.toFixed(3)}`],
        durationMs: Math.max(0, performance.now() - started),
      },
    };
  }

  // ---- Saturation fail-closed: a priority tier overflowed the shortlist budget.
  // Dropped candidates are never scored, so if the kept evidence could have been
  // outranked by an indistinguishable dropped competitor, we must NOT resolve.
  // Two cases:
  //   * shortlist_saturated_url  — dropped urlExact rows may share the winner's
  //     exact fingerprint (C4 collision). Only a tier-0 exact winner whose
  //     fingerprint has NO dropped twin is decisive.
  //   * shortlist_saturated_route — dropped route rows were ranked strictly below
  //     the kept route rows by the cheap key, so the winner is decisive only if it
  //     is not itself route-tier evidence (a dropped route row can never tie a
  //     tier-0 URL-exact winner — different canonical URL => different fingerprint).
  // ----
  if (saturatedUrl || saturatedRoute) {
    const decisiveTier0 =
      best.tier === 0 &&
      !shortlistInfo.droppedUrlFingerprints.includes(best.fingerprint);
    if (!decisiveTier0) {
      const saturatedRules: string[] = [];
      if (saturatedUrl) saturatedRules.push('shortlist_saturated_url');
      if (saturatedRoute) saturatedRules.push('shortlist_saturated_route');
      return {
        status: 'ambiguous',
        reason: 'screen_ambiguous',
        screen: null,
        matches: topMatches(sorted, maxMatches),
        telemetry: {
          ...baseTelemetry,
          rules: saturatedRules,
          durationMs: Math.max(0, performance.now() - started),
        },
      };
    }
    rules.push(
      saturatedUrl ? 'shortlist_saturated_url' : 'shortlist_saturated_route'
    );
  }

  // ---- C4 fingerprint collision: two DISTINCT numeric screen IDs share the
  // exact fingerprint. Numeric `screen_states.id` IS identity (C4); an exact
  // fingerprint is only decisive when it selects ONE row. Two rows that are
  // byte-identical across url/title/variant/auth/structure are indistinguishable
  // to every evidence tier, so guessing the lower ID would be a guess — fail
  // closed. ----
  if (best.tier === 0) {
    const distinctIds = new Set(
      eligible
        .filter((s) => s.tier === 0 && s.fingerprint === best.fingerprint)
        .map((s) => s.screenId)
    );
    if (distinctIds.size > 1) {
      return {
        status: 'ambiguous',
        reason: 'screen_ambiguous',
        screen: null,
        matches: topMatches(sorted, maxMatches),
        telemetry: {
          ...baseTelemetry,
          rules: [
            'fingerprint_collision',
            `rows=${distinctIds.size}`,
          ],
          durationMs: Math.max(0, performance.now() - started),
        },
      };
    }
  }

  // ---- Ambiguity band: same-tier competitor within the undifferentiated gap ----
  // Band competitors are the eligible rows PLUS origin-conflict rows: a foreign
  // row can never WIN (it is not electable), but a foreign structural twin that
  // ties the best same-origin row must still force the band — the title
  // tie-break below refuses to elect across an origin boundary (its
  // origin+pathname gate), so the pool fails closed exactly as it did before
  // origin-conflict rows became ineligible. Auth/stale/structure exclusions
  // keep the narrower semantics they always had (they neither win nor block).
  const bandCompetitors = sorted.filter(
    (s) => !s.authConflict && !s.structureConflict && !s.cacheStale
  );
  if (best.tier !== 0 && bandCompetitors.length >= 2) {
    const next = bandCompetitors[1];
    if (
      next.tier === best.tier &&
      best.confidence - next.confidence <= ambiguityBand
    ) {
      // R6F2: before failing closed, let a DECISIVE title elect one of the tied
      // rows. Structural twins from one template are indistinguishable on every
      // tier below the exact fingerprint, and that tier is dark whenever the
      // stored url and the live url differ by scheme, port or query — so without
      // this the twins are permanently ambiguous. `titleDecisiveCandidate`
      // states the fail-closed conditions.
      // B-12 v2: when the title axis is silent (the live title is ENTITY DATA no
      // stored row has ever seen — a freshly created record), fall through to
      // the route-SHAPE axis: the band members are usually DIFFERENT screens
      // (detail vs add vs list) and the live url's record-aware fold names
      // exactly which template it is on. Fail-closed conditions in
      // `routeShapeDecisiveCandidate`.
      const tied = bandCompetitors.filter(
        (s) => s.tier === best.tier && best.confidence - s.confidence <= ambiguityBand
      );
      const byTitle = titleDecisiveCandidate(tied, norm.title, norm.canonicalUrl, minConfidence);
      const elected = byTitle ?? routeShapeDecisiveCandidate(tied, norm.canonicalUrl, norm.title, minConfidence);
      if (elected) {
        best = elected;
        rules.push(byTitle ? 'title_discriminator' : 'route_shape_discriminator', `tied=${tied.length}`);
      } else {
        return {
          status: 'ambiguous',
          reason: 'screen_ambiguous',
          screen: null,
          matches: topMatches(sorted, maxMatches),
          telemetry: {
            ...baseTelemetry,
            rules: [
              'ambiguity_band',
              `top1=${best.confidence.toFixed(3)}`,
              `top2=${next.confidence.toFixed(3)}`,
            ],
            durationMs: Math.max(0, performance.now() - started),
          },
        };
      }
    }
  }

  // ---- Resolved ----
  // C1 derivation precedence (p0-contracts.md C1): (1) the screen's own auth
  // mode when it was entered via a known transition edge (`storedAuthMode`);
  // (2) the runtime session auth signal; (3) 'anonymous'. The stored row is the
  // transition-derived evidence, so it comes FIRST — a candidate whose screen
  // was authored as authenticated is never downgraded to anonymous just because
  // the runtime signal is unknown.
  const resolvedAuthMode: AuthModeV01 =
    best.storedAuthMode ?? norm.authSignal ?? 'anonymous';

  // `best` is never stale here (stale rows are rejected earlier), so the only
  // residual drift signal is structural.
  let driftReason: ScreenIdentityV01['driftReason'] = 'none';
  if (best.tier === 1 && best.semanticScore < STRUCTURE_DRIFT_CEIL)
    driftReason = 'structure_changed';

  rules.push(
    `tier${best.tier}`,
    best.fingerprintExact ? 'fingerprint_exact' : '',
    best.tier === 1 ? 'route_family_variant' : '',
    `matched_by=${best.matchedBy.join(',')}`
  );

  return {
    status: 'resolved',
    screen: buildIdentity(
      best,
      norm,
      resolvedAuthMode,
      driftReason,
      best.cacheVersion
    ),
    matches: topMatches(sorted, maxMatches),
    telemetry: {
      ...baseTelemetry,
      rules: rules.filter(Boolean),
      durationMs: Math.max(0, performance.now() - started),
    },
  };
}

/**
 * Convenience: resolve and return only the canonical identity outcome —
 * `{ kind: 'screen_matches', screen }` (P1 StepPreconditionV01 shape) when
 * resolved, else a canonical failure reason.
 */
export function resolveScreenIdentity(
  snapshot: ResolverSnapshotV01,
  candidates: readonly StoredScreenV01[],
  options: ResolveScreenOptionsV01 = {}
):
  | { kind: 'screen_matches'; screen: ScreenIdentityV01 }
  | { kind: 'no_match'; reason: ScreenResolutionReason } {
  const resolution = resolveScreen(snapshot, candidates, options);
  if (resolution.status === 'resolved' && resolution.screen) {
    return { kind: 'screen_matches', screen: resolution.screen };
  }
  return { kind: 'no_match', reason: resolution.reason ?? 'screen_unknown' };
}

/**
 * Browser Task Core — frozen v0.1 contract types.
 *
 * These mirror the frozen P0 v0.1 contracts in `.docs/browser-task-core/p0-contracts.md`
 * exactly (§2 request, §3 result, §4 internal IR, §5 screen identity, §6
 * pre/postconditions, §7 canonical failure/recovery reasons). Versioning rule:
 * a breaking change must bump the version and record a compatibility note in
 * `p0-contracts.md`; consumers must reject IR/request versions they do not
 * understand.
 *
 * Surface ownership: the LLM-facing surface is `BrowserTaskRequestV01` and
 * `BrowserTaskResultV01` ONLY. A model never authors `BrowserFlowIRV01`
 * (internal execution IR), selectors, action IDs, leases, profiles, auth mode,
 * cache tiers, discovery policy, planner policy, browser backend, or tab
 * choice — those are runner-owned.
 */

/** Frozen contract version for the browser-task surface. */
export const BROWSER_TASK_VERSION = '0.1' as const

export type BrowserTaskVersion = typeof BROWSER_TASK_VERSION

/**
 * The one high-level model-facing contract (P0 `p0-contracts.md` §2).
 * Intent + named data + assertions only.
 */
export interface BrowserTaskRequestV01 {
  version: '0.1'
  requestId: string
  /** Intent only. No selectors, no action IDs, no leases, no planner modes. */
  goal: string
  /** Optional. The actual current screen wins over this (P3 rule 3). */
  startUrl?: string
  /**
   * Named data slots. Values are injected at execution (P4), never inline in
   * logs/IR. Only these values may carry secrets; redaction (P1) treats every
   * data value as sensitive.
   */
  data?: Record<string, string | number | boolean>
  /** Business assertions that gate success. NOT the same as transport success. */
  assertions?: TaskAssertionV01[]
  /** Bounded budget (default runner-owned). */
  timeoutMs?: number
}

/** Task-level business assertions (P0 `p0-contracts.md` §2). */
export type TaskAssertionV01 =
  | { kind: 'text_present'; text: string }
  | { kind: 'url_pattern'; pattern: string }
  | { kind: 'element_state'; scope: string; state: 'visible' | 'enabled' | 'hidden' }
  | {
      kind: 'count'
      selectorScope: string
      op: 'eq' | 'gt' | 'gte' | 'lt' | 'lte'
      n: number
    }
  | { kind: 'custom_expression'; expression: string }

/**
 * Terminal task result (P0 `p0-contracts.md` §3).
 *
 * Every `runBrowserTask` call returns exactly this shape — never a thrown
 * error, never a partial stream. Status is the outer contract signal; `failureReason`
 * is always one canonical P0 token (no invented reason strings).
 */
export interface BrowserTaskResultV01 {
  version: '0.1'
  requestId: string
  status:
    | 'success' // all steps executed AND assertions passed
    | 'failure' // a step failed, destination mismatched, or assertion failed
    | 'ambiguous' // screen resolution failed closed (screen_ambiguous)
    | 'contract_conflict'
    | 'error' // infra or uncompileable (compiler is P3)
  stepsRun: number
  stepsTotal: number
  destination?: ScreenIdentityV01
  destinationScore?: number // final replay destination score (0..1)
  failureReason?: BrowserTaskFailureReasonV01
  recovery?: {
    attempts: number
    reasons: RecoveryReasonV01[]
  }
  metrics: {
    coreDurationMs: number // core only, excludes outer model latency
    wallDurationMs: number
    modelApiCalls: number // count of model calls during the task
    cacheHits: number
    path: ExecutionPathV01[] // ordered path actually used
    browserTabId?: string // tab identity, for same-tab/lease assertions
  }
}

export type ExecutionPathV01 =
  | 'screen_cache'
  | 'flow_graph'
  | 'live_ref'
  | 'locator'
  | 'discovery_enqueue'
  | 'scenario'
  | 'route_family'
  | 'guided'
  | 'draft_replay'

/**
 * P1 transport envelope — NOT part of the frozen v0.1 contract.
 *
 * The frozen `BrowserTaskResultV01` has no free-text field, but a
 * schema-validation failure is only useful if its issues reach the caller.
 * `browserTaskImpl` therefore returns a transport envelope whose `result` is
 * the exact frozen §3 terminal result and whose optional additive `errorText`
 * carries human-readable issue lines. `errorText` is deliberately OUTSIDE the
 * frozen contract — it never extends `BrowserTaskResultV01`, so every surfaced
 * response validates cleanly against the frozen shape.
 */
export interface BrowserTaskToolResponseV01 {
  /** The frozen §3 terminal result. Always present. */
  result: BrowserTaskResultV01
  /** P1 transport-level human-readable issue lines; never part of the contract. */
  errorText?: string[]
}

/**
 * Compiled, deterministic, versioned internal execution IR (P0 §4).
 * Produced by the P3 task compiler; NEVER authored by the LLM.
 */
export interface BrowserFlowIRV01 {
  version: '0.1'
  irId: string // deterministic hash of (request, startScreen, scenario/graph ref)
  scenario?: {
    scenarioId: string
    scenarioVersion: string
  } // optional per C2 — no Scenario storage exists yet
  startScreen: ScreenIdentityV01
  expectedFinalScreen?: ScreenIdentityV01
  steps: FlowIRStepV01[]
  requiredDataSlots: string[]
  optionalDataSlots: string[]
  authMode: 'anonymous' | 'authenticated'
  idempotency: 'read' | 'idempotent_mutation' | 'stateful_mutation'
  recoveryPolicy: 'none' | 'single_retry_read' | 'bounded'
  compileConfidence: number // 0..1
  sourceEvidence: ('scenario' | 'graph_path' | 'route_family' | 'fallback')[]
}

export interface FlowIRStepV01 {
  index: number
  action: {
    type:
      | 'navigate'
      | 'click'
      | 'fill'
      | 'type'
      | 'press'
      | 'select'
      | 'hover'
      | 'upload'
      | 'wait'
      | 'assert'
  }
  target?: {
    scope: 'screen' | 'element'
    ref?: string // semantic live ref (resolve at exec)
    role?: string // semantic role, resolved to locator candidates
    /**
     * Recorded accessible name, slot-TOKENIZED where the learner saw data
     * ('Open $company — $email'). B-8: when this template renders to this
     * run's `data`, an exact single live-name match overrides the positional
     * `ref` — list order/size drift no longer re-binds the step onto a
     * different business object.
     */
    name?: string
    locatorCandidates?: string[] // ordered candidates, never secrets
  }
  value?: DataSlotBindingV01
  preconditions?: StepPreconditionV01[]
  postconditions?: StepPostconditionV01[]
  timeoutMs: number
  mutation: boolean
}

export type DataSlotBindingV01 =
  | { kind: 'slot'; slotName: string } // value injected from request.data at exec
  | { kind: 'literal_bound'; value: string } // only non-secret literal (e.g. 'click')

export type AuthModeV01 = 'anonymous' | 'authenticated'

/**
 * Screen identity (P0 §5). `screenId` is the numeric `screen_states.id` (C4);
 * `stateKey` is `screen_states.stateId`, diagnostic and NOT unique.
 */
export interface ScreenIdentityV01 {
  screenId: number
  stateKey: string
  stateVariant?: string // same-URL SPA discriminator (P2 rule 4)
  url: string
  routeFamily?: string | null
  confidence: number // 0..1; locate_screen best_match.score
  matchedBy: (
    | 'fingerprint'
    | 'route_family'
    | 'semantic_signature'
    | 'live_ref'
  )[]
  cacheVersion?: string // semanticSignature backfill/schema version
  authMode?: AuthModeV01 // C1: derived, not stored
  driftReason?: 'stale_fingerprint' | 'structure_changed' | 'none'
}

export type StepPreconditionV01 =
  | { kind: 'screen_matches'; screen: ScreenIdentityV01 }
  | { kind: 'data_slot_bound'; slotName: string }
  | { kind: 'element_present'; scope: string }
  | { kind: 'auth_state'; authMode: AuthModeV01 }
  | { kind: 'no_mutation_conflict' }

/**
 * Step postcondition union (P0 §6 / C3 reconciliation). `computeReplayVerdict`'s
 * scored transition-end postcondition maps onto `destination_screen_score`;
 * graph-chain `VerificationCondition` maps onto `url_pattern` / `element_state`
 * / `text_present` / `assertion`. No third shape is invented.
 */
export type StepPostconditionV01 =
  | { kind: 'destination_screen_score'; minScore: number } // from computeReplayVerdict
  | { kind: 'url_pattern'; pattern: string }
  | { kind: 'element_state'; scope: string; state: 'visible' | 'enabled' | 'hidden' }
  | { kind: 'text_present'; text: string }
  | { kind: 'assertion'; ref: string } // references an assertion in the request

/**
 * Canonical failure reasons (P0 §7). Every `failureReason` is one of these
 * stable tokens — never an invented string.
 */
export type BrowserTaskFailureReasonV01 =
  // plan_flow (lib/flow-planner.ts)
  | 'no_path'
  | 'start_unknown'
  | 'goal_unknown'
  // graph-chain planner (lib/graph-chain-planner.ts)
  | 'no_transitions'
  | 'no_start_node'
  | 'no_target_transition'
  | 'no_valid_path'
  | 'missing_data_slots'
  // replay verdict (lib/mcp-tools.ts computeReplayVerdict)
  | 'step_failed'
  | 'no_expected_destination'
  | 'destination_mismatch'
  // extension bridge / runtime (lib/extension-bridge.ts)
  | 'no_extension'
  | 'extension_timeout'
  | 'scope_denied'
  | 'daemon_unreachable'
  // screen resolver (P2)
  | 'screen_ambiguous'
  | 'screen_unknown'
  | 'screen_stale'
  // task compiler (P3)
  | 'no_scenario'
  | 'contract_conflict'
  | 'missing_data'
  // infra
  | 'error'

export type RecoveryReasonV01 =
  // runtime-discovery reasons (lib/runtime-discovery.ts)
  | 'db_miss'
  | 'locator_ready'
  | 'locator_ref_only'
  | 'locator_partial'
  | 'locator_missing'
  | 'locator_unknown'
  | 'replay_failure'
  | 'planner_no_path'
  | 'planner_guard_failed'
  | 'planner_verification_failed'
  | 'planner_missing_data_slots'
  // bounded recovery classification (P6)
  | 'retry_read'
  | 'replanned_path'
  | 'recovered_to_discovery'
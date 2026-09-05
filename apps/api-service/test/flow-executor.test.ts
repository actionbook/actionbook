/**
 * P4 — Deterministic flow executor fail-first tests (test/flow-executor.test.ts).
 *
 * Drives `executeFlow` against a FAKE `ExecutorRuntimeV01` (no DB, no browser, no
 * extension). Removing any behavior under test makes the corresponding assertion
 * fail ("fail-first"). Each behavior maps to one P4 requirement:
 *
 *   1.  unsupported / malformed IR fails before dispatch        (validateFlowIr)
 *   1b. executor rejects an unrunnable step before any dispatch (plan reject)
 *   2.  missing data slot fails before mutation
 *   3.  exact semantic click beats a broad clickable cached locator
 *   4.  broad click locator with a semantic mismatch is rejected
 *   5.  fill/type use the correct named field + exact bound value
 *   6.  select/press/hover/navigate/wait verb mapping (exact extension args)
 *   7.  stabilization wait after a mutation is bounded, never unbounded
 *   8.  slow save/login/submit transitions get a LARGER bounded budget
 *   9.  a failed mutation stops later mutations
 *  10.  a stateful mutation failure is never retried / never switches candidate
 *  11.  read/idempotent retry is bounded and deterministic (policy-capped)
 *  14.  mid-flow tab/lease drift fails closed before the next mutation
 *  15.  reversed locator/ref input order → identical dispatch decision
 *  17.  secret canaries absent from notes / telemetry / onStep events
 *  18.  oversized values + upload fail safely, before any mutation
 *  19.  a throwing runtime → terminal structured result, never an escaped throw
 *  20.  transport OK alone never becomes status:'success' (destination +
 *       assertions gates)
 */

import { describe, it, expect } from 'vitest'
import * as fs from 'node:fs'
import * as path from 'node:path'
import {
  executeFlow,
  validateFlowIr,
  acceptableRoles,
  interpretSemanticRefValueProbe,
  findLiveRef,
  nameAnchoredLiveRef,
  FLOW_EXECUTOR_LIMITS,
  type ExecutorRuntimeV01,
  type FlowDispatchResultV01,
  type LiveRefNodeV01,
  type RuntimeAcquireResultV01,
} from '../lib/flow-executor'
import { planTargetResolution, normalizeLocatorCandidates } from '../lib/dispatch-order'
import type {
  BrowserFlowIRV01,
  BrowserTaskRequestV01,
  DataSlotBindingV01,
  FlowIRStepV01,
  ScreenIdentityV01,
} from '../types/browser-task'
import type { IrActionKindV01 } from '../lib/ir'
import type {
  ResolverSnapshotV01,
  ScreenResolutionV01,
  ScreenResolutionReason,
  ScreenResolutionStatus,
} from '../lib/screen-resolver'

// ---------------------------------------------------------------------------
// Fixture helpers (pure — no I/O).
// ---------------------------------------------------------------------------

const START_URL = 'https://fixture.test/42'
const DEST_URL = 'https://fixture.test/77'

function screen(screenId: number, stateKey?: string): ScreenIdentityV01 {
  return {
    screenId,
    stateKey: stateKey ?? `s${screenId}`,
    url: `https://fixture.test/${screenId}`,
    routeFamily: 'app',
    confidence: 0.95,
    matchedBy: ['fingerprint'],
    authMode: 'anonymous',
    driftReason: 'none',
  }
}

function resolution(
  status: ScreenResolutionStatus,
  scr: ScreenIdentityV01 | null,
  reason?: ScreenResolutionReason
): ScreenResolutionV01 {
  return {
    status,
    ...(reason ? { reason } : {}),
    screen: scr,
    matches: scr
      ? [
          {
            screenId: scr.screenId,
            stateKey: scr.stateKey,
            url: scr.url,
            routeFamily: scr.routeFamily ?? null,
            confidence: scr.confidence,
            matchedBy: scr.matchedBy,
            tier: status === 'resolved' ? 0 : 3,
            semanticScore: status === 'resolved' ? 1 : 0,
            liveRefScore: 0,
            fingerprintExact: status === 'resolved',
            authConflict: false,
            structureConflict: false,
            originConflict: false,
            originUnprovable: false,
            cacheStale: false,
            cacheUnversioned: false,
          },
        ]
      : [],
    telemetry: {
      inputTruncated: false,
      inputByteSize: 0,
      candidateCount: 1,
      poolTotal: 1,
      shortlistBudget: 100,
      shortlistKept: 1,
      shortlistDropped: 0,
      shortlistByUrl: 1,
      shortlistByRoute: 0,
      shortlistByFallback: 0,
      shortlistSaturatedTier: null,
      rules: [],
      durationMs: 0,
    },
  }
}

function baseSnapshot(url: string): ResolverSnapshotV01 {
  return { url, title: null, headings: [], landmarkRoles: [], forms: [] }
}

const MUTATION_ACTIONS: ReadonlySet<IrActionKindV01> = new Set([
  'navigate',
  'click',
  'fill',
  'type',
  'press',
  'select',
  'hover',
  'upload',
])

function isMutationAction(action: IrActionKindV01): boolean {
  return MUTATION_ACTIONS.has(action)
}

function slot(name: string): DataSlotBindingV01 {
  return { kind: 'slot', slotName: name }
}

function literal(value: string): DataSlotBindingV01 {
  return { kind: 'literal_bound', value }
}

function makeRequest(data?: Record<string, string | number | boolean>): BrowserTaskRequestV01 {
  return {
    version: '0.1',
    requestId: 'ft-req-1',
    goal: 'open the detail page',
    ...(data ? { data } : {}),
  }
}

function makeIr(
  steps: FlowIRStepV01[],
  opts: { startScreen?: ScreenIdentityV01; expectedFinalScreen?: ScreenIdentityV01 | null } = {}
): BrowserFlowIRV01 {
  return {
    version: '0.1',
    irId: 'test-ir-00000000000000000000000000000000',
    scenario: { scenarioId: 'test-scenario', scenarioVersion: '0.1' },
    startScreen: opts.startScreen ?? screen(42),
    // `null` means "no expected destination" (omitted from the IR, so the
    // `!ir.expectedFinalScreen` gate fires); `undefined` defaults to screen 77.
    ...(opts.expectedFinalScreen !== undefined
      ? { expectedFinalScreen: opts.expectedFinalScreen ?? undefined }
      : { expectedFinalScreen: screen(77) }),
    steps,
    requiredDataSlots: [],
    optionalDataSlots: [],
    authMode: 'anonymous',
    idempotency: 'idempotent_mutation',
    recoveryPolicy: 'none',
    compileConfidence: 0.9,
    sourceEvidence: ['scenario'],
  }
}

interface ElemStepOpts {
  ref?: string
  role?: string
  /** B-8: recorded accessible name — may carry $slot tokens. */
  name?: string
  locatorCandidates?: string[]
  value?: DataSlotBindingV01
  mutation?: boolean
  timeoutMs?: number
}

/** Element-scoped step (the P3 compiler's typical emission). */
function elemStep(index: number, action: IrActionKindV01, opts: ElemStepOpts = {}): FlowIRStepV01 {
  const ref =
    opts.ref ??
    (opts.locatorCandidates ? undefined : `${opts.role ?? 'element'}@${index}`)
  const target: FlowIRStepV01['target'] = {
    scope: 'element',
    ...(opts.role ? { role: opts.role } : {}),
    ...(ref ? { ref } : {}),
    ...(opts.name ? { name: opts.name } : {}),
    ...(opts.locatorCandidates ? { locatorCandidates: opts.locatorCandidates } : {}),
  }
  return {
    index,
    action: { type: action },
    target,
    ...(opts.value ? { value: opts.value } : {}),
    mutation: opts.mutation ?? isMutationAction(action),
    timeoutMs: opts.timeoutMs ?? 8000,
  }
}

/** Screen-scoped step (navigate / wait / assert). */
function screenStep(index: number, action: IrActionKindV01, ref: string, timeoutMs = 8000): FlowIRStepV01 {
  return {
    index,
    action: { type: action },
    target: { scope: 'screen', ref },
    mutation: isMutationAction(action),
    timeoutMs,
  }
}

// ---------------------------------------------------------------------------
// Fake runtime (deterministic; records every interaction).
// ---------------------------------------------------------------------------

class FakeRuntime implements ExecutorRuntimeV01 {
  nowMs = 1_000_000
  binding = { tabId: 'tab-1', sessionId: 'sess-1', leaseId: 'lease-1', identityKey: 'ext:tab-1' }

  acquireCalls = 0
  acquireResult: RuntimeAcquireResultV01 = { ok: true, binding: this.binding }
  verifyOk = true
  verifyCalls = 0
  /** Pre-landing live snapshot (what the page shows before any mutation). */
  syncDefault: ResolverSnapshotV01
  /** Post-landing live snapshot (what the page shows after a mutation lands). */
  destSnapshot: ResolverSnapshotV01
  /**
   * Model of the real page: sync returns `syncDefault` (START, resolves to the
   * start screen) until the first mutation lands, then returns `destSnapshot`
   * (DEST, resolves to the expected destination). In `neverStable` mode the page
   * keeps flickering after landing so waitForStable's bounded loop is what
   * terminates, never a stability streak.
   */
  landed = false
  neverStable = false
  /**
   * B-15: scripted post-landing snapshots — while non-empty after landing,
   * shifted in order instead of `destSnapshot`. Models a navigation commit
   * that lands N polls late (stale-but-stable pre-nav page in between). Sized
   * to survive `waitForStable` (3 polls) with entries left for the final gate.
   */
  postLandingQueue: ResolverSnapshotV01[] = []
  /**
   * B-15: when true, the page never navigates after landing (stale forever) —
   * the convergence window must expire bounded and fail closed as before.
   */
  neverNavigate = false
  syncCalls = 0
  dispatchLog: { actionType: string; args: Record<string, unknown> }[] = []
  dispatchHandler: (
    actionType: string,
    args: Record<string, unknown>
  ) => Promise<FlowDispatchResultV01> = async (actionType, args) => {
    // P5 (rework round 2, finding 1): field write-verification is DEFAULT ON,
    // so after every locator-dispatched valued mutation the executor issues a
    // read-only `selectorValue` field check. A page that applied the write answers
    // it with the canonical probe-ok envelope — this keeps the default honest
    // without forcing every test to re-implement the model. Tests that need a
    // page that CANNOT confirm the write override this handler.
    if (actionType === 'selectorValue') {
      return { ok: true, result: { value: { ok: true } } }
    }
    // P5 (rework round 4, finding 1): a semantic-ref valued write is verified
    // by the identity-bound `readRefValue` verb — the extension resolves the
    // backendDOMNodeId the action rode (DOM.resolveNode + Runtime.callFunctionOn)
    // and compares the value on THAT node. The default fake models the real
    // page: it confirms the probe ONLY for a refId it saw receive the value via
    // performActionWithRef AND whose modelled value matches — never a canned
    // {ok:true} for any ref (that was the round-3 false-positive shape).
    if (actionType === 'readRefValue') {
      return this.readRefValueHandler(args)
    }
    return { ok: true, result: {} }
  }
  refNodes: LiveRefNodeV01[] = []
  refsCalls = 0
  performLog: { refId: string; method: string; args: unknown[] }[] = []
  performResult: FlowDispatchResultV01 = { ok: true, result: {} }
  /**
   * P5 rework round 4, finding 1 — modelled DOM values per refId. A
   * performActionWithRef fill seeds the modelled value for that refId; a
   * seed entry overrides it (to model a page where the field already holds a
   * value). `null` models a field that holds no value.
   */
  refValues: Record<string, string | null> = {}
  /** Every identity-bound probe the executor dispatched (round 4, finding 1). */
  readRefValueProbeLog: { refId: string; expect: unknown; match: unknown }[] = []
  /** The same-node identity model behind the default `readRefValue` answer. */
  readRefValueHandler = async (args: Record<string, unknown>): Promise<FlowDispatchResultV01> => {
    const refId = String(args.refId ?? '')
    this.readRefValueProbeLog.push({ refId, expect: args.expect, match: args.match })
    const current = this.refValues[refId]
    const expected = String(args.expect ?? '')
    const match = args.match === 'contains' ? 'contains' : 'equals'
    const ok = current !== null && current !== undefined &&
      (match === 'contains' ? current.includes(expected) : current === expected)
    return {
      ok: true,
      // The LIVE extension router envelope (background.js `readRefValue`
      // case): boolean-only verdict, never echoes the field value. Kept
      // byte-identical to what the shipped extension answers — the contract
      // test below locks both sides against drift.
      result: { refId, verified: ok, match },
    }
  }
  screenByUrl: Record<string, number> = { [START_URL]: 42, [DEST_URL]: 77 }
  classifyHandler: (snapshot: ResolverSnapshotV01) => ScreenResolutionV01

  constructor(syncDefault = baseSnapshot(START_URL), destSnapshot = baseSnapshot(DEST_URL)) {
    this.syncDefault = syncDefault
    this.destSnapshot = destSnapshot
    this.classifyHandler = (snap) => {
      const id = this.screenByUrl[snap.url] ?? 42
      return resolution('resolved', screen(id))
    }
  }

  now(): number {
    return this.nowMs
  }

  async acquire(): Promise<RuntimeAcquireResultV01> {
    this.acquireCalls++
    return this.acquireResult
  }

  async verifyBinding(): Promise<boolean> {
    this.verifyCalls++
    return this.verifyOk
  }

  async syncScreen(): Promise<ResolverSnapshotV01> {
    this.syncCalls++
    // Advance the clock a little per sync so bounded stabilization budgets are
    // provably finite even when the live snapshot never stabilizes.
    this.nowMs += 40
    if (!this.landed) return this.syncDefault
    if (this.neverNavigate) return this.syncDefault
    if (this.postLandingQueue.length > 0) return this.postLandingQueue.shift()!
    if (this.neverStable) {
      // Alternate DEST/START forever: no stability streak, the budget is the
      // only thing that can stop the stabilization loop.
      return this.syncCalls % 2 === 0 ? this.destSnapshot : this.syncDefault
    }
    return this.destSnapshot
  }

  classifyScreen(snapshot: ResolverSnapshotV01): ScreenResolutionV01 {
    return this.classifyHandler(snapshot)
  }

  async snapshotRefs(): Promise<LiveRefNodeV01[]> {
    this.refsCalls++
    return this.refNodes
  }

  async performActionWithRef(refId: string, method: string, args: unknown[]): Promise<FlowDispatchResultV01> {
    this.performLog.push({ refId, method, args })
    const out = await this.performResult
    if (out.ok) {
      // Round 4, finding 1: a successful valued mutation seeds the SAME node's
      // modelled value, so an identity-bound readRefValue on THIS refId confirms
      // the write while a probe bound to any other node sees no value. A denied
      // mutation (performResult ok:false) seeds nothing. `select` lands the
      // option the action matched (value OR label — the same shape the page
      // op's select branch and round-6 control-aware read-back model).
      if (method === 'fill' || method === 'type' || method === 'select') {
        this.refValues[refId] = String(args[0] ?? '')
      }
      this.landed = true
    }
    return out
  }

  async dispatch(actionType: string, args: Record<string, unknown>): Promise<FlowDispatchResultV01> {
    this.dispatchLog.push({ actionType, args })
    const out = await this.dispatchHandler(actionType, args)
    if (out.ok) this.landed = true
    return out
  }
}

function run(request: BrowserTaskRequestV01, ir: BrowserFlowIRV01, rt: FakeRuntime) {
  return executeFlow(request, ir, rt, {
    secrets: request.data ? Object.values(request.data).map(String) : [],
    sleep: async (ms) => {
      rt.nowMs += ms
    },
  })
}

// ---------------------------------------------------------------------------
// 1. Unsupported / malformed IR fails before dispatch (validateFlowIr).
// ---------------------------------------------------------------------------

describe('1 — validateFlowIr fails closed before any dispatch', () => {
  const good = elemStep(0, 'click', { locatorCandidates: ['[data-testid="go"]'] })
  const goodIr = makeIr([good])

  it('rejects a non-0.1 version', () => {
    const out = validateFlowIr({ ...goodIr, version: '9.9' }, {})
    expect(out.ok).toBe(false)
    if (!out.ok) expect(out.reason).toBe('contract_conflict')
  })

  it('rejects more steps than the bounded maximum', () => {
    const steps = Array.from({ length: FLOW_EXECUTOR_LIMITS.stepsMaxTotal + 1 }, (_, i) =>
      elemStep(i, 'click', { locatorCandidates: [`#c${i}`] })
    )
    const out = validateFlowIr(makeIr(steps), {})
    expect(out.ok).toBe(false)
  })

  it('rejects a non-contiguous step index', () => {
    const bad = { ...elemStep(0, 'click', { locatorCandidates: ['#a'] }), index: 5 }
    const out = validateFlowIr(makeIr([bad]), {})
    expect(out.ok).toBe(false)
    if (!out.ok) expect(out.detail).toContain('non-contiguous index')
  })

  it('rejects an unknown action type', () => {
    const bad = { ...elemStep(0, 'click', { locatorCandidates: ['#a'] }) }
    ;(bad.action as { type: string }).type = 'inject'
    const out = validateFlowIr(makeIr([bad]), {})
    expect(out.ok).toBe(false)
    if (!out.ok) expect(out.detail).toContain('unknown action type')
  })

  it('rejects a non-finite / negative timeoutMs', () => {
    const bad = { ...elemStep(0, 'click', { locatorCandidates: ['#a'] }), timeoutMs: -5 }
    const out = validateFlowIr(makeIr([bad]), {})
    expect(out.ok).toBe(false)
    if (!out.ok) expect(out.detail).toContain('invalid timeoutMs')
  })

  it('rejects a navigate step with an element-scoped target', () => {
    const bad = elemStep(0, 'navigate', { role: 'button', ref: 'button@0' })
    const out = validateFlowIr(makeIr([bad]), {})
    expect(out.ok).toBe(false)
    if (!out.ok) expect(out.detail).toContain('navigate step 0 requires a screen-scoped target')
  })

  it('rejects a value on a non-value-capable action', () => {
    const bad = elemStep(0, 'click', { locatorCandidates: ['#a'], value: literal('x') })
    const out = validateFlowIr(makeIr([bad]), {})
    expect(out.ok).toBe(false)
  })

  it('rejects a press step without a bound key', () => {
    const bad = elemStep(0, 'press', { locatorCandidates: ['#enter'] })
    const out = validateFlowIr(makeIr([bad]), {})
    expect(out.ok).toBe(false)
    if (!out.ok) expect(out.detail).toContain('press step 0 requires a bound key')
  })

  // --- P5 rework round 4, finding 2: the mutation flag must agree with the
  // value-writing action kind, and the flag is a required boolean field. The
  // round-4 probe ran a fill IR crafted with mutation:false and got
  // `success` with no write read-back — the verification gate keyed on the
  // unvalidated flag. The validator now rejects the mismatch, and the gate
  // keys on action kind (second line of defense). ---

  it('round 4 finding 2: rejects a fill marked mutation:false', () => {
    const bad = elemStep(0, 'fill', { locatorCandidates: ['#name'], value: literal('x'), mutation: false })
    const out = validateFlowIr(makeIr([bad]), {})
    expect(out.ok).toBe(false)
    if (!out.ok) {
      expect(out.reason).toBe('contract_conflict')
      expect(out.detail).toContain('must be marked mutation:true')
    }
  })

  it('round 4 finding 2: rejects a type marked mutation:false', () => {
    const bad = elemStep(0, 'type', { locatorCandidates: ['#note'], value: literal('x'), mutation: false })
    const out = validateFlowIr(makeIr([bad]), {})
    expect(out.ok).toBe(false)
    if (!out.ok) expect(out.detail).toContain('must be marked mutation:true')
  })

  it('round 4 finding 2: rejects a select marked mutation:false', () => {
    const bad = elemStep(0, 'select', { locatorCandidates: ['#plan'], value: literal('pro'), mutation: false })
    const out = validateFlowIr(makeIr([bad]), {})
    expect(out.ok).toBe(false)
    if (!out.ok) expect(out.detail).toContain('must be marked mutation:true')
  })

  it('round 4 finding 2: rejects a valued write with a missing (non-boolean) mutation flag', () => {
    const bad = { ...elemStep(0, 'fill', { locatorCandidates: ['#name'], value: literal('x') }) }
    delete (bad as { mutation?: boolean }).mutation
    const out = validateFlowIr(makeIr([bad]), {})
    expect(out.ok).toBe(false)
    if (!out.ok) expect(out.detail).toContain('missing or non-boolean mutation flag')
  })

  it('round 4 finding 2: a click may remain mutation:false (click is not a valued write)', () => {
    // The read/idempotent retry policy legitimately marks idempotent clicks
    // non-mutating; only value-writing kinds are forced to mutation:true.
    const ok = { ...elemStep(0, 'click', { locatorCandidates: ['#a'] }), mutation: false }
    const out = validateFlowIr(makeIr([ok]), {})
    expect(out.ok).toBe(true)
  })

  it('round 4 finding 2: a valued write with mutation:true is accepted', () => {
    const ok = elemStep(0, 'fill', { locatorCandidates: ['#name'], value: literal('x'), mutation: true })
    const out = validateFlowIr(makeIr([ok]), {})
    expect(out.ok).toBe(true)
  })
})

describe('1a — executeFlow itself rejects malformed IR before binding or dispatching (round 5, finding 2)', () => {
  it('a direct executeFlow() call on a fill marked mutation:false binds nothing and dispatches nothing', async () => {
    // The round-5 empirical probe: calling executeFlow() DIRECTLY (the
    // production runner validates at browser-task-runner, but the exported
    // executor's contract must be closed on its own) used to return
    // {"status":"failure","stepsRun":1,"dispatches":["fill","selectorValue"]} —
    // the fill had ALREADY MUTATED the page before the failure surfaced.
    // The executor now validates the IR itself before binding a tab.
    const bad = elemStep(0, 'fill', { locatorCandidates: ['#name'], value: literal('leak-canary-r5f2'), mutation: false })
    const rt = new FakeRuntime()
    const out = await run(makeRequest(), makeIr([bad]), rt)
    expect(out.result.status).toBe('contract_conflict')
    expect(out.result.failureReason).toBe('contract_conflict')
    expect(out.result.stepsRun).toBe(0) // was 1 with a live fill before the fix
    expect(rt.dispatchLog).toHaveLength(0)
    expect(rt.performLog).toHaveLength(0)
    expect(rt.acquireCalls).toBe(0) // no tab bound for an invalid IR
    expect(String(out.note ?? '')).toContain('must be marked mutation:true')
    expect(String(out.note ?? '')).toContain('nothing was executed and nothing was mutated')
    expect(String(out.note ?? '')).not.toContain('leak-canary-r5f2') // no value leak
  })
})

describe('1c — round 6: valued writes never land on checked-state controls', () => {
  it('a checked-state control is never an acceptable target for fill/type/select', () => {
    // The round-6 false-positive's executor-side guard: the semantic match
    // itself must not resolve a fill onto a checkbox.
    expect(acceptableRoles('checkbox', 'fill').has('checkbox')).toBe(false)
    expect(acceptableRoles('checkbox', 'type').has('checkbox')).toBe(false)
    expect(acceptableRoles('radio', 'select').has('radio')).toBe(false)
    expect(acceptableRoles('switch', 'fill').has('switch')).toBe(false)
    // Toggling by click still matches — checked-state controls are clicked.
    expect(acceptableRoles('toggle_field', 'click').has('checkbox')).toBe(true)
    expect(acceptableRoles('checkbox', 'click').has('checkbox')).toBe(true)
    // Text controls keep their valued-write match.
    expect(acceptableRoles('text_field', 'fill').has('textbox')).toBe(true)
    expect(acceptableRoles(undefined, 'fill').has('textbox')).toBe(true)
  })

  it('a fill whose only live target is a checkbox fails closed before any dispatch', async () => {
    // The page has exactly ONE fillable-looking node and it is a checkbox:
    // the executor must refuse the write pre-dispatch (no performActionWithRef,
    // no probe), not confirm it through a transport success.
    const ir = makeIr([
      elemStep(0, 'fill', { role: 'text_field', ref: 'text_field@0', value: literal('true') }),
    ])
    const rt = new FakeRuntime()
    rt.refNodes = [{ refId: 'ref-agree', role: 'checkbox', name: 'Agree' }]
    const out = await run(makeRequest(), ir, rt)
    expect(out.result.status).toBe('failure')
    expect(out.result.failureReason).toBe('step_failed')
    expect(out.result.stepsRun).toBe(0)
    expect(rt.performLog).toHaveLength(0) // the valued write never rode a ref
    expect(rt.readRefValueProbeLog).toHaveLength(0) // nothing was read back
    expect(rt.dispatchLog).toHaveLength(0)
    expect(String(out.note ?? '')).toContain('no live semantic target')
    expect(String(out.note ?? '')).toContain('failed closed before mutating')
  })

  it('a select on a semantic combobox target: success, and the read-back probes the same value as the action', async () => {
    // Round 6, finding 2 (false-negative): the select ACTION matches an option
    // by value OR label, so the identity-bound read-back must carry the SAME
    // expectation the action landed — a label-form write ("Premium") is
    // confirmed by the control-aware page op (selected option value OR label),
    // not by a raw element.value string compare.
    const ir = makeIr([
      elemStep(0, 'select', { role: 'combobox', ref: 'combobox@0', value: slot('plan') }),
    ])
    const rt = new FakeRuntime()
    rt.refNodes = [{ refId: 'ref-plan', role: 'combobox', name: 'Plan' }]
    const out = await run(makeRequest({ plan: 'Premium' }), ir, rt)
    expect(out.result.status).toBe('success')
    expect(rt.performLog).toHaveLength(1)
    expect(rt.performLog[0]).toMatchObject({ refId: 'ref-plan', method: 'select', args: ['Premium'] })
    // The identity-bound probe is bound to the SAME node and carries the SAME
    // value the action used — equals (never contains) for a select.
    expect(rt.readRefValueProbeLog).toHaveLength(1)
    expect(rt.readRefValueProbeLog[0]).toEqual({ refId: 'ref-plan', expect: 'Premium', match: 'equals' })
    expect(String(out.note ?? '')).not.toContain('Premium') // no value leak
  })

  it('a select that reports transport success but whose same-ref read mismatches fails closed', async () => {
    // Defense in depth: even if a valued write rode a checked-state-like
    // target (or the page swallowed it), a read-back mismatch is NEVER a
    // success — transport OK must not become status:'success'.
    const ir = makeIr([
      elemStep(0, 'select', { role: 'combobox', ref: 'combobox@0', value: slot('plan') }),
    ])
    const rt = new FakeRuntime()
    rt.refNodes = [{ refId: 'ref-plan', role: 'combobox', name: 'Plan' }]
    // The page's SAME node reads back a DIFFERENT value than dispatched (the
    // select action landed but the page coerced it to another option): the
    // identity-bound probe must answer a definite mismatch.
    rt.dispatchHandler = async (actionType, args) => {
      if (actionType === 'readRefValue') {
        rt.readRefValueProbeLog.push({
          refId: String(args.refId ?? ''),
          expect: args.expect,
          match: args.match,
        })
        return { ok: true, result: { value: { ok: false, code: 'VALUE_MISMATCH' } } }
      }
      return { ok: true, result: {} }
    }
    const out = await run(makeRequest({ plan: 'Premium' }), ir, rt)
    expect(out.result.status).toBe('failure')
    expect(out.result.failureReason).toBe('step_failed')
    expect(out.result.stepsRun).toBe(1) // the select ran; the read-back refused it
    expect(rt.readRefValueProbeLog).toHaveLength(1) // the probe DID run and saw the mismatch
    expect(rt.readRefValueProbeLog[0]).toEqual({ refId: 'ref-plan', expect: 'Premium', match: 'equals' })
    expect(String(out.note ?? '')).toContain('does not hold the dispatched value')
  })
})

describe('1b — upload dispatches (registered fixture) and fails closed otherwise', () => {
  it('dispatches structured uploadFile for a registered fixture on a precise locator', async () => {
    const rt = new FakeRuntime()
    rt.dispatchHandler = async (actionType) =>
      actionType === 'uploadFile'
        ? { ok: true, result: { value: { ok: true } } }
        : { ok: true, result: {} }
    const ir = makeIr([
      elemStep(0, 'upload', { locatorCandidates: ['#file'], value: slot('file') }),
    ])
    const out = await run(makeRequest({ file: 'resume.pdf' }), ir, rt)
    expect(out.result.status).toBe('success')
    expect(out.result.stepsRun).toBe(1)
    expect(rt.dispatchLog).toHaveLength(1)
    expect(rt.dispatchLog[0].actionType).toBe('uploadFile')
    expect(rt.dispatchLog[0].args).toMatchObject({ selector: '#file', filename: 'resume.pdf', mimeType: 'application/pdf' })
    expect(typeof rt.dispatchLog[0].args.base64).toBe('string')
  })

  it('rejects an unregistered fixture path at plan time before any mutation', async () => {
    const rt = new FakeRuntime()
    const ir = makeIr([
      elemStep(0, 'upload', { locatorCandidates: ['#file'], value: slot('file') }),
    ])
    const out = await run(makeRequest({ file: 'unknown.docx' }), ir, rt)
    expect(out.result.status).toBe('failure')
    expect(out.result.failureReason).toBe('step_failed')
    expect(out.result.stepsRun).toBe(0)
    // Fail-first: not one browser verb, not even a screen sync, happened.
    expect(rt.dispatchLog).toHaveLength(0)
    expect(rt.performLog).toHaveLength(0)
    expect(rt.verifyCalls).toBe(0)
    expect(rt.syncCalls).toBe(0)
  })

  it('fails the step closed when the page reports the file input could not be set', async () => {
    const rt = new FakeRuntime()
    rt.dispatchHandler = async (actionType) =>
      actionType === 'uploadFile'
        ? {
            ok: true,
            result: {
              value: {
                ok: false,
                code: 'NO_FILE_INPUT',
                reason: 'no file input matched the locator',
              },
              world: 'MAIN',
            },
          }
        : { ok: true, result: {} }
    const ir = makeIr([
      elemStep(0, 'upload', { locatorCandidates: ['#file'], value: slot('file') }),
    ])
    const out = await run(makeRequest({ file: 'resume.pdf' }), ir, rt)
    expect(out.result.status).toBe('failure')
    expect(out.result.failureReason).toBe('step_failed')
    expect(out.result.stepsRun).toBe(0)
    // The structured uploadFile operation WAS dispatched (a real browser call), but the page-level
    // `{ ok:false }` is a step failure — never reported as success.
    expect(rt.dispatchLog).toHaveLength(1)
    expect(rt.dispatchLog[0].actionType).toBe('uploadFile')
  })

  it('routes a compiled fill on file_field@N (no locator) to uploadFile by position', async () => {
    // The real fixture-upload template compiles to a `fill` on a semantic
    // `file_field@N` ref with NO stored locator. That must become an upload
    // uploadFile (Nth file input by position), never a performActionWithRef
    // 'fill' — a silent no-op on a file input.
    const rt = new FakeRuntime()
    rt.dispatchHandler = async (actionType) =>
      actionType === 'uploadFile'
        ? { ok: true, result: { value: { ok: true } } }
        : { ok: true, result: {} }
    const ir = makeIr([
      elemStep(0, 'fill', { role: 'file_field', ref: 'file_field@0', value: slot('file') }),
    ])
    const out = await run(makeRequest({ file: 'resume.pdf' }), ir, rt)
    expect(out.result.status).toBe('success')
    expect(out.result.stepsRun).toBe(1)
    expect(rt.dispatchLog).toHaveLength(1)
    expect(rt.dispatchLog[0].actionType).toBe('uploadFile')
    expect(rt.performLog).toHaveLength(0)
    expect(rt.dispatchLog[0].args).toMatchObject({ semanticIndex: 0, filename: 'resume.pdf', mimeType: 'application/pdf' })
    expect(rt.dispatchLog[0].args).not.toHaveProperty('selector')
    // P5 finding 1: the uploadFile operation's own page-ok IS the write proof, so
    // default-on field verification dispatches NO follow-up check.
    expect(rt.dispatchLog).toHaveLength(1)
  })

  it('rejects an unregistered fixture on a file_field fill before any mutation', async () => {
    const rt = new FakeRuntime()
    const ir = makeIr([
      elemStep(0, 'fill', { role: 'file_field', ref: 'file_field@0', value: slot('file') }),
    ])
    const out = await run(makeRequest({ file: 'unknown.docx' }), ir, rt)
    expect(out.result.status).toBe('failure')
    expect(out.result.failureReason).toBe('step_failed')
    expect(out.result.stepsRun).toBe(0)
    expect(rt.dispatchLog).toHaveLength(0)
    expect(rt.performLog).toHaveLength(0)
    expect(rt.verifyCalls).toBe(0)
  })
})

// ---------------------------------------------------------------------------
// 2. Missing data slot fails before mutation.
// ---------------------------------------------------------------------------

describe('2 — missing data slot fails before mutation', () => {
  const ir = makeIr([elemStep(0, 'fill', { role: 'text_field', ref: 'text_field@0', value: slot('email') })])

  it('validateFlowIr reports missing_data when the slot is absent', () => {
    const out = validateFlowIr(ir, {})
    expect(out.ok).toBe(false)
    if (!out.ok) {
      expect(out.reason).toBe('missing_data')
      expect(out.detail).toContain('"email"')
    }
  })

  it('executor stops the step before any browser interaction when the slot is missing', async () => {
    const rt = new FakeRuntime()
    const out = await run(makeRequest(), ir, rt)
    // Round 5, finding 2: executeFlow now validates the IR itself BEFORE
    // binding a tab or dispatching anything — the unbound value slot is
    // refused at the entry (missing_data), never at step 0.
    expect(out.result.failureReason).toBe('missing_data')
    expect(out.result.stepsRun).toBe(0)
    expect(rt.dispatchLog).toHaveLength(0)
    expect(rt.performLog).toHaveLength(0)
    expect(rt.acquireCalls).toBe(0) // no tab bound for an invalid IR
    expect(out.note ?? '').toContain('email')
    expect(out.note ?? '').toContain('nothing was executed and nothing was mutated')
  })
})

// ---------------------------------------------------------------------------
// 3. Exact semantic click beats a broad clickable cached locator.
// ---------------------------------------------------------------------------

describe('3 — semantic click beats a broad clickable cached locator', () => {
  const ir = makeIr([
    elemStep(0, 'click', { role: 'auth_entry', ref: 'auth_entry@0', locatorCandidates: ['button'] }),
  ])

  it('plans a semantic_ref even when a broad cached locator is present', () => {
    const plan = planTargetResolution(ir.steps[0], {})
    expect(plan.kind).toBe('semantic_ref')
    expect(plan.category).toBe('semantic_ref')
    expect(plan.semanticRole).toBe('auth_entry')
  })

  it('executes via performActionWithRef on the matched live node, never the broad locator', async () => {
    const rt = new FakeRuntime()
    rt.refNodes = [{ refId: 'ref-submit', role: 'button', name: 'Submit' }]
    const out = await run(makeRequest(), ir, rt)
    expect(out.result.status).toBe('success')
    expect(rt.performLog).toHaveLength(1)
    expect(rt.performLog[0]).toMatchObject({ refId: 'ref-submit', method: 'click', args: [] })
    // Fail-first: the broad "button" locator must NOT be dispatched verbatim.
    expect(rt.dispatchLog.filter((d) => d.actionType === 'click')).toHaveLength(0)
  })
})

// ---------------------------------------------------------------------------
// 4. Broad click locator with a semantic mismatch is rejected.
// ---------------------------------------------------------------------------

describe('4 — broad locator requires a live semantic confirmation', () => {
  const ir = makeIr([elemStep(0, 'click', { role: 'button', locatorCandidates: ['button'] })])

  it('plans a validated_locator (never a blind dispatch) for a broad selector', () => {
    const plan = planTargetResolution(ir.steps[0], {})
    expect(plan.kind).toBe('validated_locator')
    expect(plan.locator).toBe('button')
  })

  it('fails closed when no live node matches the semantic role', async () => {
    const rt = new FakeRuntime()
    rt.refNodes = [{ refId: 'ref-link', role: 'link', name: 'Something else' }]
    const out = await run(makeRequest(), ir, rt)
    expect(out.result.failureReason).toBe('step_failed')
    expect(out.result.stepsRun).toBe(0)
    expect(rt.dispatchLog.filter((d) => d.actionType === 'click')).toHaveLength(0)
    expect(out.note ?? '').toContain('no matching live')
  })

  it('dispatches only after a live node confirms the role', async () => {
    const rt = new FakeRuntime()
    rt.refNodes = [{ refId: 'ref-button', role: 'button', name: 'Go' }]
    const out = await run(makeRequest(), ir, rt)
    expect(out.result.status).toBe('success')
    expect(rt.dispatchLog.filter((d) => d.actionType === 'click')).toHaveLength(1)
    expect(rt.dispatchLog[0].args).toMatchObject({ selector: 'button' })
  })
})

// ---------------------------------------------------------------------------
// 5. fill/type use the correct named field + exact bound value.
// ---------------------------------------------------------------------------

describe('5 — valued actions bind the exact named slot value', () => {
  it('fills the named field with the exact slot value and never leaks it', async () => {
    const ir = makeIr([
      elemStep(0, 'fill', { role: 'text_field', ref: 'text_field@0', value: slot('email') }),
    ])
    const rt = new FakeRuntime()
    rt.refNodes = [{ refId: 'ref-email', role: 'textbox', name: 'Email' }]
    const data = { email: 'boss@fixture.test' }
    const out = await run(makeRequest(data), ir, rt)
    expect(out.result.status).toBe('success')
    expect(rt.performLog).toHaveLength(1)
    expect(rt.performLog[0]).toMatchObject({ refId: 'ref-email', method: 'fill' })
    expect(rt.performLog[0].args).toEqual(['boss@fixture.test'])
    // Secret hygiene: the value reaches the bound runtime but never the outcome.
    expect(JSON.stringify(out.result)).not.toContain('boss@fixture.test')
    expect(out.note ?? '').not.toContain('boss@fixture.test')
  })

  it('P5 finding 1: a plain fill -> click flow without assertions verifies the write before the click (default on)', async () => {
    // The production runner does not opt in to field verification and the
    // compiler only attaches a postcondition to the LAST transition step, so
    // this exact shape was the reviewer's exploit: an opt-in gate ran the
    // click before confirming the fill. Default-on must fail the fill closed
    // — and never execute the click — when the page cannot confirm it.
    const ir = makeIr([
      elemStep(0, 'fill', { role: 'text_field', ref: 'text_field@0', locatorCandidates: ['#name'], value: literal('alpha') }),
      elemStep(1, 'click', { role: 'primary_button', ref: 'primary_button@0', locatorCandidates: ['#save'] }),
    ])
    const rt = new FakeRuntime()
    // A page that silently drops the field probe: no boolean confirmation.
    rt.dispatchHandler = async () => ({ ok: true, result: {} })
    const out = await run(makeRequest(), ir, rt)
    expect(out.result.status).toBe('failure')
    expect(out.result.failureReason).toBe('step_failed')
    expect(out.result.stepsRun).toBe(1) // the fill ran; the click never did
    expect(rt.dispatchLog.map((d) => d.actionType)).toEqual(['fill', 'selectorValue'])
    expect(String(out.note ?? '')).toContain('field write-verification')
  })

  it('P5 finding 1: a definite field-value mismatch also fails the fill before the click', async () => {
    const ir = makeIr([
      elemStep(0, 'fill', { role: 'text_field', ref: 'text_field@0', locatorCandidates: ['#name'], value: literal('alpha') }),
      elemStep(1, 'click', { role: 'primary_button', ref: 'primary_button@0', locatorCandidates: ['#save'] }),
    ])
    const rt = new FakeRuntime()
    // The page reports a DEFINITE different value: probe envelope ok, value
    // ok:false — a mismatch, not an unverifiable probe.
    rt.dispatchHandler = async () => ({ ok: true, result: { value: { ok: false } } })
    const out = await run(makeRequest(), ir, rt)
    expect(out.result.status).toBe('failure')
    expect(out.result.failureReason).toBe('step_failed')
    expect(out.result.stepsRun).toBe(1)
    expect(rt.dispatchLog.map((d) => d.actionType)).toEqual(['fill', 'selectorValue'])
    expect(String(out.note ?? '')).toContain('does not hold the dispatched value')
  })

  it('P5 round 3 finding 4: no option opts a flow out of write verification', async () => {
    // The round-3 finding: `verifyFieldWrites: false` publicly disabled ALL
    // write verification. The option is deleted from `FlowExecuteOptionsV01`
    // (typed tombstone `noWriteVerificationBypassExists?: never`), so a
    // production caller CANNOT ask for an unverified fill. This test pins the
    // behaviour from the type side: the same fill -> click flow that the old
    // bypass let sail through now fails closed on a page that cannot confirm
    // the write — whatever the caller passes, the gate is unconditional.
    const ir = makeIr([
      elemStep(0, 'fill', { role: 'text_field', ref: 'text_field@0', locatorCandidates: ['#name'], value: literal('alpha') }),
      elemStep(1, 'click', { role: 'primary_button', ref: 'primary_button@0', locatorCandidates: ['#save'] }),
    ])
    const rt = new FakeRuntime()
    rt.dispatchHandler = async () => ({ ok: true, result: {} })
    const out = await executeFlow(makeRequest(), ir, rt, {
      secrets: [],
      sleep: async (ms) => {
        rt.nowMs += ms
      },
      // @ts-expect-error the bypass no longer exists in the production
      // contract — a caller passing it must not get unverified writes.
      verifyFieldWrites: false,
    })
    expect(out.result.status).toBe('failure')
    expect(out.result.failureReason).toBe('step_failed')
    expect(out.result.stepsRun).toBe(1) // the fill ran; the click never did
    expect(rt.dispatchLog.map((d) => d.actionType)).toEqual(['fill', 'selectorValue'])
    expect(String(out.note ?? '')).toContain('field write-verification')
  })

  it('types into the named field via a precise locator with the exact text', async () => {
    const ir = makeIr([
      elemStep(0, 'type', { role: 'text_field', ref: 'text_field@0', locatorCandidates: ['#note'], value: literal('hello') }),
    ])
    const rt = new FakeRuntime()
    const out = await run(makeRequest(), ir, rt)
    expect(out.result.status).toBe('success')
    expect(rt.dispatchLog[0]).toMatchObject({ actionType: 'type', args: { selector: '#note', text: 'hello' } })
    // P5 finding 1: default-on field write-verification follows the dispatch
    // with a read-only locator probe BEFORE any next mutation can run.
    expect(rt.dispatchLog[1].actionType).toBe('selectorValue')
    expect(rt.dispatchLog[1].args).toMatchObject({ selector: '#note', expected: 'hello', match: 'contains' })
  })

  it('P5 round 4 finding 1: a semantic-ref fill is re-read on the SAME backend node before the click (identity-bound readRefValue)', async () => {
    // Round 3 pinned a DOM-order read-back; round 4 (finding 1,
    // CRITICAL) showed that probe re-selected the target via DOM order and was
    // fooled by a hidden aria-hidden clone holding a pre-existing value. The
    // executor now dispatches the identity-bound `readRefValue` verb — the
    // extension resolves the backendDOMNodeId the fill RODE (actedRefId) and
    // compares the value on THAT node. The fake confirms ONLY because the
    // perform seeded ref-name's modelled value with the dispatched text.
    const ir = makeIr([
      elemStep(0, 'fill', { role: 'text_field', ref: 'text_field@0', value: literal('alpha') }),
      elemStep(1, 'click', { role: 'primary_button', ref: 'primary_button@0', locatorCandidates: ['#save'] }),
    ])
    const rt = new FakeRuntime()
    rt.refNodes = [{ refId: 'ref-name', role: 'textbox', name: 'Name' }]
    const out = await run(makeRequest(), ir, rt)
    expect(out.result.status).toBe('success')
    // The fill landed via the semantic ref (performActionWithRef), then an
    // identity-bound read-back ran BEFORE the click — transport ok alone never
    // became the click, and no DOM-order selector probe was dispatched.
    const probe = rt.dispatchLog.find((d) => d.actionType === 'readRefValue')
    expect(probe).toBeDefined()
    expect(rt.dispatchLog.some((d) => d.actionType === 'evaluate')).toBe(false)
    // The probe binds the SAME node the fill rode, with the exact dispatched
    // value and the equals match (fill/select compare with equals).
    expect(probe!.args).toMatchObject({ refId: 'ref-name', expect: 'alpha', match: 'equals' })
    // The read-back sits between the fill and the click, not after it.
    const order = rt.dispatchLog.map((d) => d.actionType)
    expect(order.indexOf('readRefValue')).toBeLessThan(order.indexOf('click'))
    // The fake's same-node model answered ok because the perform seeded
    // ref-name — NOT because of a canned {ok:true}.
    expect(rt.refValues['ref-name']).toBe('alpha')
  })

  it('P5 round 4 finding 1: an aria-hidden clone holding the value can never green the real node (same-node identity)', async () => {
    // The reviewer's empirical probe: the real field stayed empty while a
    // DIFFERENT hidden aria-hidden field with a pre-existing value passed a
    // DOM-order probe (`domProbeTarget:'hidden aria-hidden'`,
    // `probeResult:{ok:true}`). With identity-bound read-back, a probe on the
    // real node can only see the real node's modelled value — the clone's
    // pre-existing value is invisible to it.
    const ir = makeIr([
      elemStep(0, 'fill', { role: 'text_field', ref: 'text_field@0', value: literal('alpha') }),
      elemStep(1, 'click', { role: 'primary_button', ref: 'primary_button@0', locatorCandidates: ['#save'] }),
    ])
    const rt = new FakeRuntime()
    rt.refNodes = [{ refId: 'ref-name', role: 'textbox', name: 'Name' }]
    // The clone field already holds the expected value — but the fill rides
    // ref-name, and the fake models the write as silently dropped (perform
    // succeeds at the transport level, no value lands on the real node).
    rt.refValues = { 'ref-hidden-clone': 'alpha' }
    rt.performResult = { ok: true, result: {} }
    rt.performActionWithRef = async (refId, method, args) => {
      rt.performLog.push({ refId, method, args })
      // Transport ok, but the real page DROPPED the insertText: no seed.
      return { ok: true, result: {} }
    }
    const out = await run(makeRequest(), ir, rt)
    // Fail closed: the identity-bound probe saw the REAL node was empty, even
    // though a hidden clone held the value. The round-3 DOM-order probe would
    // have returned {ok:true} here.
    expect(out.result.status).toBe('failure')
    expect(out.result.failureReason).toBe('step_failed')
    expect(out.result.stepsRun).toBe(1) // the fill ran; the click never did
    expect(rt.performLog).toHaveLength(1)
    expect(rt.dispatchLog.map((d) => d.actionType)).toEqual(['readRefValue'])
    expect(rt.readRefValueProbeLog).toEqual([{ refId: 'ref-name', expect: 'alpha', match: 'equals' }])
    expect(String(out.note ?? '')).toContain('does not hold the dispatched value')
  })

  it('P5 round 4 finding 1: a type step probes with the contains match', async () => {
    const ir = makeIr([
      elemStep(0, 'type', { role: 'text_field', ref: 'text_field@0', value: literal('partial') }),
    ])
    const rt = new FakeRuntime()
    rt.refNodes = [{ refId: 'ref-note', role: 'textbox', name: 'Note' }]
    rt.refValues = { 'ref-note': 'a partial sentence' }
    const out = await run(makeRequest(), ir, rt)
    expect(out.result.status).toBe('success')
    expect(rt.readRefValueProbeLog).toEqual([{ refId: 'ref-note', expect: 'partial', match: 'contains' }])
  })

  it('P5 round 4 finding 1: a silent semantic-ref page (no read-back) fails the fill before the click', async () => {
    // The exact round-3 exploit, re-pinned for the identity-bound verb: the
    // extension runs Input.insertText and returns ok without reading the value
    // back. Here the fake page cannot confirm the write (empty probe envelope)
    // — the fill must fail closed and the next mutation must never run.
    const ir = makeIr([
      elemStep(0, 'fill', { role: 'text_field', ref: 'text_field@0', value: literal('alpha') }),
      elemStep(1, 'click', { role: 'primary_button', ref: 'primary_button@0', locatorCandidates: ['#save'] }),
    ])
    const rt = new FakeRuntime()
    rt.refNodes = [{ refId: 'ref-name', role: 'textbox', name: 'Name' }]
    rt.dispatchHandler = async (actionType) =>
      actionType === 'readRefValue' ? { ok: true, result: {} } : { ok: true, result: {} }
    const out = await run(makeRequest(), ir, rt)
    expect(out.result.status).toBe('failure')
    expect(out.result.failureReason).toBe('step_failed')
    expect(out.result.stepsRun).toBe(1) // the fill ran; the click never did
    // The semantic-ref fill rode the live-ref seam (performActionWithRef),
    // then the identity-bound read-back ran and could not confirm the write.
    expect(rt.performLog).toHaveLength(1)
    expect(rt.dispatchLog.map((d) => d.actionType)).toEqual(['readRefValue'])
    expect(String(out.note ?? '')).toContain('field write-verification')
  })

  it('P5 round 4 finding 1: a definite same-node value mismatch fails closed', async () => {
    const ir = makeIr([
      elemStep(0, 'fill', { role: 'text_field', ref: 'text_field@0', value: literal('alpha') }),
      elemStep(1, 'click', { role: 'primary_button', ref: 'primary_button@0', locatorCandidates: ['#save'] }),
    ])
    const rt = new FakeRuntime()
    rt.refNodes = [{ refId: 'ref-name', role: 'textbox', name: 'Name' }]
    // Definite mismatch: the SAME node read back a different value.
    rt.dispatchHandler = async (actionType) =>
      actionType === 'readRefValue'
        ? { ok: true, result: { value: { ok: false, code: 'VALUE_MISMATCH' } } }
        : { ok: true, result: {} }
    const out = await run(makeRequest(), ir, rt)
    expect(out.result.status).toBe('failure')
    expect(out.result.failureReason).toBe('step_failed')
    expect(out.result.stepsRun).toBe(1)
    expect(rt.performLog).toHaveLength(1)
    expect(rt.dispatchLog.map((d) => d.actionType)).toEqual(['readRefValue'])
    expect(String(out.note ?? '')).toContain('does not hold the dispatched value')
  })

  it('P5 round 4 finding 1: a transport-rejected readRefValue envelope fails closed — never a false-positive', async () => {
    // A transport-level rejection of the read-back itself (e.g. the ref fell
    // out of the extension's ref cache mid-task and the router answers
    // ok:false / REF_NOT_FOUND). That is NOT a page fact and NOT a mismatch —
    // the gate must read it as UNVERIFIABLE (PROBE_NO_IDENTITY_VERB) and fail
    // closed, never as a confirmed write.
    const ir = makeIr([
      elemStep(0, 'fill', { role: 'text_field', ref: 'text_field@0', value: literal('alpha') }),
      elemStep(1, 'click', { role: 'primary_button', ref: 'primary_button@0', locatorCandidates: ['#save'] }),
    ])
    const rt = new FakeRuntime()
    rt.refNodes = [{ refId: 'ref-name', role: 'textbox', name: 'Name' }]
    rt.dispatchHandler = async (actionType) =>
      actionType === 'readRefValue'
        ? { ok: false, error: { code: 'UNKNOWN_ACTION', message: 'unknown verb readRefValue' } }
        : { ok: true, result: {} }
    const out = await run(makeRequest(), ir, rt)
    expect(out.result.status).toBe('failure')
    expect(out.result.failureReason).toBe('step_failed')
    expect(out.result.stepsRun).toBe(1) // the fill ran; the click never did
    expect(String(out.note ?? '')).toContain('PROBE_NO_IDENTITY_VERB')
    expect(String(out.note ?? '')).not.toContain('alpha') // no value leak
  })

  it('P5 round 4 finding 1: a readRefValue dispatch that THROWS is a mechanism failure (PROBE_DISPATCH_FAILED), still fail-closed', async () => {
    const ir = makeIr([
      elemStep(0, 'fill', { role: 'text_field', ref: 'text_field@0', value: literal('alpha') }),
    ])
    const rt = new FakeRuntime()
    rt.refNodes = [{ refId: 'ref-name', role: 'textbox', name: 'Name' }]
    rt.dispatchHandler = async (actionType) => {
      if (actionType === 'readRefValue') throw new Error('bridge dropped mid-probe')
      return { ok: true, result: {} }
    }
    const out = await run(makeRequest(), ir, rt)
    expect(out.result.status).toBe('failure')
    expect(out.result.failureReason).toBe('step_failed')
    expect(String(out.note ?? '')).toContain('PROBE_DISPATCH_FAILED')
    expect(String(out.note ?? '')).not.toContain('alpha')
  })

  it('P5 round 5 finding 4: a semantic-ref write with an EMPTY value fails closed before ANY readRefValue dispatch', async () => {
    // A `match:'contains'` probe with an empty expectation is VACUOUS —
    // ''.includes('') is true on every node, so the verb would fake a
    // confirmed write. The locator path already blocks this with
    // PROBE_EMPTY_VALUE; the semantic path enforces the same bound up front.
    const ir = makeIr([
      elemStep(0, 'type', { role: 'text_field', ref: 'text_field@0', value: literal('') }),
    ])
    const rt = new FakeRuntime()
    rt.refNodes = [{ refId: 'ref-note', role: 'textbox', name: 'Note' }]
    const out = await run(makeRequest(), ir, rt)
    expect(out.result.status).toBe('failure')
    expect(out.result.failureReason).toBe('step_failed')
    expect(out.result.stepsRun).toBe(1)
    // The guard fires in verifySemanticWrite BEFORE the probe verb is ever
    // dispatched — no readRefValue left the executor.
    expect(rt.dispatchLog.map((d) => d.actionType)).toEqual([])
    expect(rt.readRefValueProbeLog).toHaveLength(0)
    expect(String(out.note ?? '')).toContain('PROBE_EMPTY_VALUE')
  })
})

// ---------------------------------------------------------------------------
// 6. Verb mapping — exact extension args for select/press/hover/navigate/wait.
// ---------------------------------------------------------------------------

describe('6 — verb dispatch args match the extension contract', () => {
  it('drives a multi-verb flow in IR order with exact per-verb args', async () => {
    const steps: FlowIRStepV01[] = [
      elemStep(0, 'select', { locatorCandidates: ['#plan'], value: slot('plan') }),
      elemStep(1, 'press', { locatorCandidates: ['#enter'], value: slot('key') }),
      elemStep(2, 'hover', { locatorCandidates: ['#menu'] }),
      screenStep(3, 'navigate', DEST_URL),
      screenStep(4, 'wait', 'https://fixture.test/', 500),
    ]
    const ir = makeIr(steps)
    const rt = new FakeRuntime()
    // Field write-verification is UNCONDITIONAL (round 3, finding 4: no bypass
    // exists). It is covered in §5; here it adds one read-only `selectorValue`
    // probe right after the valued `select` dispatch. The verb-contract
    // assertions below account for it explicitly.
    const out = await executeFlow(makeRequest({ plan: 'pro', key: 'Enter' }), ir, rt, {
      secrets: ['pro', 'Enter'],
      sleep: async (ms) => {
        rt.nowMs += ms
      },
    })
    expect(out.result.status).toBe('success')
    expect(out.result.stepsRun).toBe(5)
    const actions = rt.dispatchLog.map((d) => d.actionType)
    // The valued select is followed by its write-verification selectorValue probe
    // before the next mutation; press/hover/navigate are not valued.
    expect(actions).toEqual(['select', 'selectorValue', 'press', 'hover', 'navigate'])
    expect(rt.dispatchLog[0].args).toEqual({ selector: '#plan', value: 'pro' })
    expect(rt.dispatchLog[1].actionType).toBe('selectorValue')
    expect(rt.dispatchLog[1].args).toMatchObject({ selector: '#plan', expected: 'pro', match: 'equals' })
    expect(rt.dispatchLog[2].args).toEqual({ key: 'Enter', selector: '#enter' })
    expect(rt.dispatchLog[3].args).toEqual({ selector: '#menu' })
    expect(rt.dispatchLog[4].args).toEqual({ url: DEST_URL })
    // Path reflects the routed categories.
    expect(out.result.metrics.path).toContain('locator')
    expect(out.result.metrics.path).toContain('route_family')
  })
})

// ---------------------------------------------------------------------------
// 7. Stabilization after a mutation is bounded, never unbounded.
// ---------------------------------------------------------------------------

describe('7 — post-mutation stabilization is bounded', () => {
  it('terminates even when the live snapshot never stabilizes', async () => {
    const ir = makeIr([elemStep(0, 'click', { role: 'button', ref: 'button@0' })])
    const rt = new FakeRuntime()
    rt.refNodes = [{ refId: 'ref-button', role: 'button', name: 'Go' }]
    // Never-stable: the live snapshot alternates so waitForStable can never
    // reach a stability streak — the loop must still end within its budget.
    rt.neverStable = true
    const out = await run(makeRequest(), ir, rt)
    expect(out.result.stepsRun).toBe(1)
    // Fail-first: the stabilization loop MUST terminate within its bounded
    // budget even though the live snapshot never stabilizes. If waitForStable
    // were unbounded, syncCalls would explode and the flow would hang.
    expect(rt.syncCalls).toBeLessThan(120)
    // The flow reached a terminal verdict — it never escaped to a hang. The
    // exact terminal depends on the final-sync parity under the flicker, which
    // is incidental to the boundedness being proven here.
    expect(['success', 'failure', 'error', 'ambiguous']).toContain(out.result.status)
  })
})

// ---------------------------------------------------------------------------
// 8. Slow save/login/submit transitions get a LARGER bounded budget.
// ---------------------------------------------------------------------------

describe('8 — slow transitions get a larger bounded stabilization budget', () => {
  async function stabilizeCalls(role: string): Promise<number> {
    // timeoutMs far above the slow budget (15s) so the slow transition is NOT
    // capped by the step deadline — the isSlowTransition budget is what bounds
    // the stabilization loop.
    const ir = makeIr([elemStep(0, 'click', { role, ref: `${role}@0`, timeoutMs: 20000 })])
    const rt = new FakeRuntime()
    rt.refNodes = [{ refId: `ref-${role}`, role, name: role }]
    // Never-stable snapshot forces waitForStable to exhaust its full budget.
    rt.neverStable = true
    await run(makeRequest(), ir, rt)
    return rt.syncCalls
  }

  it('gives submit/login/save transitions more stabilization time than a plain click', async () => {
    const plain = await stabilizeCalls('button')
    const submit = await stabilizeCalls('submit_action')
    // standardStabilizeMs 3000 ≈ ~13 syncs; slowStabilizeMs 15000 ≈ ~54. The
    // slow-transition budget must be materially larger — and both must be
    // bounded (this is what "larger bounded budget" means).
    expect(submit).toBeGreaterThan(plain * 3)
    expect(submit).toBeLessThan(plain * 8)
  })
})

// ---------------------------------------------------------------------------
// 9. A failed mutation stops later mutations.
// ---------------------------------------------------------------------------

describe('9 — failure stops the flow and later mutations never execute', () => {
  it('stops after the first failed mutation', async () => {
    const steps: FlowIRStepV01[] = [
      elemStep(0, 'click', { locatorCandidates: ['#a'] }),
      elemStep(1, 'fill', { locatorCandidates: ['#b'], value: slot('v') }),
      elemStep(2, 'click', { locatorCandidates: ['#c'] }),
    ]
    const ir = makeIr(steps)
    const rt = new FakeRuntime()
    rt.dispatchHandler = async (actionType) =>
      actionType === 'fill'
        ? { ok: false, error: { code: 'EXTENSION_TIMEOUT', message: 'bridge stalled' } }
        : { ok: true, result: {} }
    const out = await run(makeRequest({ v: 'x' }), ir, rt)
    expect(out.result.stepsRun).toBe(1)
    expect(out.result.failureReason).toBe('extension_timeout')
    // Steps 0 and 1 ran; step 2 was never dispatched.
    expect(rt.dispatchLog.map((d) => d.actionType)).toEqual(['click', 'fill'])
    expect(out.note ?? '').toContain('stopped')
  })
})

// ---------------------------------------------------------------------------
// 10. Stateful mutation failures are never retried / never switch candidates.
// ---------------------------------------------------------------------------

describe('10 — a stateful mutation failure is never retried', () => {
  it('dispatches a failing mutation exactly once, on the first candidate only', async () => {
    const ir = makeIr([
      elemStep(0, 'fill', { locatorCandidates: ['#a', '#b'], value: slot('v') }),
    ])
    const rt = new FakeRuntime()
    rt.dispatchHandler = async () => ({ ok: false, error: { code: 'EXTENSION_TIMEOUT', message: 'stalled' } })
    const out = await run(makeRequest({ v: 'x' }), ir, rt)
    expect(out.result.failureReason).toBe('extension_timeout')
    const fills = rt.dispatchLog.filter((d) => d.actionType === 'fill')
    // Even though EXTENSION_TIMEOUT is a transient code, mutations are NEVER
    // retried and the candidate is NEVER switched.
    expect(fills).toHaveLength(1)
    expect(fills[0].args).toMatchObject({ selector: '#a' })
  })
})

// ---------------------------------------------------------------------------
// 11. Read/idempotent retry is bounded and deterministic.
// ---------------------------------------------------------------------------

describe('11 — read/idempotent retry is bounded and deterministic', () => {
  function retryIr(policy: BrowserFlowIRV01['recoveryPolicy']): BrowserFlowIRV01 {
    // A read/idempotent dispatch step (mutation:false) lets the retry policy
    // apply to a transient dispatch failure.
    const step = { ...elemStep(0, 'click', { locatorCandidates: ['#x'] }), mutation: false }
    return { ...makeIr([step]), recoveryPolicy: policy }
  }

  it('retries exactly the policy-capped number of times on transient codes', async () => {
    for (const [policy, attempts] of [
      ['none', 1],
      ['single_retry_read', 2],
      ['bounded', 3],
    ] as const) {
      const ir = retryIr(policy)
      const rt = new FakeRuntime()
      rt.dispatchHandler = async () => ({ ok: false, error: { code: 'NO_EXTENSION', message: 'gone' } })
      const out = await run(makeRequest(), ir, rt)
      expect(out.result.status).toBe('error')
      expect(rt.dispatchLog).toHaveLength(attempts)
      // recovery is only surfaced when at least one retry happened (attempts>0).
      expect(out.result.recovery?.attempts ?? 0).toBe(attempts - 1)
    }
  })

  it('does not retry a non-transient failure at all', async () => {
    const ir = retryIr('bounded')
    const rt = new FakeRuntime()
    rt.dispatchHandler = async () => ({ ok: false, error: { code: 'REF_NOT_FOUND', message: 'no ref' } })
    const out = await run(makeRequest(), ir, rt)
    expect(out.result.failureReason).toBe('step_failed')
    expect(rt.dispatchLog).toHaveLength(1)
    expect(out.result.recovery).toBeUndefined()
  })
})

// ---------------------------------------------------------------------------
// 14. Mid-flow tab/lease drift fails closed before the next mutation.
// ---------------------------------------------------------------------------

describe('14 — mid-flow binding drift fails closed', () => {
  it('verifies the binding before every mutation and stops when it drifts', async () => {
    const steps: FlowIRStepV01[] = [
      elemStep(0, 'click', { locatorCandidates: ['#a'] }),
      elemStep(1, 'click', { locatorCandidates: ['#b'] }),
    ]
    const ir = makeIr(steps)
    const rt = new FakeRuntime()
    rt.verifyOk = true
    rt.dispatchHandler = async () => {
      // Simulate the connection/tab being released after the first mutation.
      rt.verifyOk = false
      return { ok: true, result: {} }
    }
    const out = await run(makeRequest(), ir, rt)
    expect(out.result.failureReason).toBe('error')
    expect(out.result.stepsRun).toBe(1)
    // Step 1 was never dispatched.
    expect(rt.dispatchLog.map((d) => d.actionType)).toEqual(['click'])
    expect(out.note ?? '').toContain('binding drifted')
  })
})

// ---------------------------------------------------------------------------
// 15. Reversed locator/ref input order → identical dispatch decision.
// ---------------------------------------------------------------------------

describe('15 — dispatch decisions are input-order independent', () => {
  it('normalizes candidate lists so reversed input order yields the same pick', () => {
    const a = ['[data-testid="save"]', 'button', '[data-testid="cancel"]']
    const b = [...a].reverse()
    expect(normalizeLocatorCandidates(a)).toEqual(normalizeLocatorCandidates(b))
  })

  it('plans the SAME verb/locator for reversed candidate order', () => {
    const base: FlowIRStepV01 = {
      index: 0,
      action: { type: 'click' },
      target: { scope: 'element', role: 'button', locatorCandidates: ['[data-testid="go"]', '#main > button'] },
      mutation: true,
      timeoutMs: 8000,
    }
    const forward = planTargetResolution(base, {})
    const reversed = planTargetResolution({ ...base, target: { ...base.target!, locatorCandidates: [...(base.target!.locatorCandidates ?? [])].reverse() } }, {})
    expect(forward.kind).toBe(reversed.kind)
    expect(forward.category).toBe(reversed.category)
    expect(forward.verb).toBe(reversed.verb)
    expect(forward.locator).toBe(reversed.locator)
    expect(forward.locatorCandidates).toEqual(reversed.locatorCandidates)
  })
})

// ---------------------------------------------------------------------------
// 17. Secret canaries absent from notes / telemetry / onStep events.
// ---------------------------------------------------------------------------

describe('17 — secrets never reach notes, telemetry, or onStep events', () => {
  it('redacts data values from every surfaced artifact', async () => {
    const ir = makeIr([
      elemStep(0, 'fill', { role: 'text_field', ref: 'text_field@0', value: slot('email') }),
      elemStep(1, 'click', { role: 'auth_entry', ref: 'auth_entry@0' }),
    ])
    const rt = new FakeRuntime()
    rt.refNodes = [
      { refId: 'ref-email', role: 'textbox', name: 'Email' },
      { refId: 'ref-submit', role: 'button', name: 'Submit' },
    ]
    const secrets = ['boss@fixture.test', 'hunter2!']
    const data = { email: 'boss@fixture.test', password: 'hunter2!' }
    const events: { action: string; category: string }[] = []
    const out = await executeFlow(makeRequest(data), ir, rt, {
      secrets,
      sleep: async (ms) => {
        rt.nowMs += ms
      },
      onStep: (evt) => events.push({ action: evt.action, category: evt.category }),
    })
    expect(out.result.status).toBe('success')
    const serialized = JSON.stringify(out) + JSON.stringify(events)
    for (const secret of secrets) {
      expect(serialized).not.toContain(secret)
    }
  })
})

// ---------------------------------------------------------------------------
// 18. Oversized values + upload fail safely before any mutation.
// ---------------------------------------------------------------------------

describe('18 — oversized values and uploads fail safely', () => {
  it('rejects a literal value above the byte bound', () => {
    const ir = makeIr([elemStep(0, 'fill', { role: 'f', ref: 'f@0', value: literal('x'.repeat(FLOW_EXECUTOR_LIMITS.valueBytesMax + 1)) })])
    const out = validateFlowIr(ir, {})
    expect(out.ok).toBe(false)
    if (!out.ok) expect(out.detail).toContain('exceeds')
  })

  it('rejects a slot-resolved value above the byte bound', () => {
    const ir = makeIr([elemStep(0, 'fill', { role: 'f', ref: 'f@0', value: slot('blob') })])
    const out = validateFlowIr(ir, { blob: 'y'.repeat(FLOW_EXECUTOR_LIMITS.valueBytesMax + 1) })
    expect(out.ok).toBe(false)
    if (!out.ok) expect(out.detail).toContain('exceeds')
  })

  it('accepts a value on an upload step (the fixture path slot/literal)', () => {
    const ir = makeIr([elemStep(0, 'upload', { role: 'f', ref: 'f@0', value: literal('resume.pdf') })])
    const out = validateFlowIr(ir, {})
    expect(out.ok).toBe(true)
  })

  it('rejects an oversized value on an upload step', () => {
    const ir = makeIr([
      elemStep(0, 'upload', { role: 'f', ref: 'f@0', value: literal('x'.repeat(FLOW_EXECUTOR_LIMITS.valueBytesMax + 1)) }),
    ])
    const out = validateFlowIr(ir, {})
    expect(out.ok).toBe(false)
    if (!out.ok) expect(out.detail).toContain('exceeds')
  })
})

// ---------------------------------------------------------------------------
// 19. A throwing runtime → terminal structured result, never an escaped throw.
// ---------------------------------------------------------------------------

describe('19 — runtime exceptions become terminal outcomes, never throws', () => {
  async function runThrowing(ir: BrowserFlowIRV01, rt: FakeRuntime): Promise<{ status: string; reason?: string }> {
    let outcome: Awaited<ReturnType<typeof executeFlow>>
    try {
      outcome = await run(makeRequest(), ir, rt)
    } catch {
      throw new Error('executeFlow must never reject on a throwing runtime')
    }
    return { status: outcome.result.status, reason: outcome.result.failureReason }
  }

  it('acquire throwing → error terminal', async () => {
    const rt = new FakeRuntime()
    ;(rt as unknown as { acquire: () => Promise<RuntimeAcquireResultV01> }).acquire = async () => {
      throw new Error('bridge down')
    }
    const { status, reason } = await runThrowing(makeIr([elemStep(0, 'click', { locatorCandidates: ['#a'] })]), rt)
    expect(status).toBe('error')
    expect(reason).toBe('error')
  })

  it('syncScreen throwing during a mutation gate → error terminal', async () => {
    const rt = new FakeRuntime()
    ;(rt as unknown as { syncScreen: () => Promise<ResolverSnapshotV01> }).syncScreen = async () => {
      throw new Error('bridge dead')
    }
    const { status, reason } = await runThrowing(makeIr([elemStep(0, 'click', { locatorCandidates: ['#a'] })]), rt)
    expect(status).toBe('error')
    expect(reason).toBe('error')
  })

  it('dispatch throwing → error terminal', async () => {
    const rt = new FakeRuntime()
    rt.dispatchHandler = async () => {
      throw new Error('boom')
    }
    const { status, reason } = await runThrowing(makeIr([elemStep(0, 'click', { locatorCandidates: ['#a'] })]), rt)
    expect(status).toBe('error')
    expect(reason).toBe('error')
  })

  it('snapshotRefs throwing on a semantic_ref step → error terminal', async () => {
    const rt = new FakeRuntime()
    ;(rt as unknown as { snapshotRefs: () => Promise<LiveRefNodeV01[]> }).snapshotRefs = async () => {
      throw new Error('ax tree dead')
    }
    const { status, reason } = await runThrowing(makeIr([elemStep(0, 'click', { role: 'button', ref: 'button@0' })]), rt)
    expect(status).toBe('error')
    expect(reason).toBe('error')
  })

  it('classifyScreen throwing → error terminal', async () => {
    const rt = new FakeRuntime()
    rt.classifyHandler = () => {
      throw new Error('resolver dead')
    }
    const { status, reason } = await runThrowing(makeIr([elemStep(0, 'click', { locatorCandidates: ['#a'] })]), rt)
    expect(status).toBe('error')
    expect(reason).toBe('error')
  })
})

// ---------------------------------------------------------------------------
// 20. Transport OK alone never becomes status:'success'.
// ---------------------------------------------------------------------------

describe('20 — transport completion alone is never green', () => {
  const happy = makeIr([elemStep(0, 'click', { locatorCandidates: ['#a'] })])

  async function transportOk(ir: BrowserFlowIRV01, rt: FakeRuntime, request = makeRequest()) {
    return run(request, ir, rt)
  }

  it('resolves to success ONLY when the final screen matches the expected destination', async () => {
    const rt = new FakeRuntime()
    const out = await transportOk(happy, rt)
    expect(out.result.status).toBe('success')
    expect(out.result.stepsRun).toBe(happy.steps.length)
  })

  it('a non-resolved FINAL screen → ambiguous, never success', async () => {
    const rt = new FakeRuntime()
    // The step-0 gate resolves (START → 42); ONLY the post-mutation final
    // classify fails to resolve — proving the destination gate is the guard.
    rt.classifyHandler = (snap) =>
      snap.url === DEST_URL
        ? resolution('ambiguous', null, 'screen_ambiguous')
        : resolution('resolved', screen(42))
    const out = await transportOk(happy, rt)
    expect(out.result.status).toBe('ambiguous')
    expect(out.result.failureReason).toBe('screen_ambiguous')
  })

  it('a final screen on the WRONG screen → destination_mismatch', async () => {
    const rt = new FakeRuntime()
    rt.screenByUrl = { [DEST_URL]: 999 }
    const out = await transportOk(happy, rt)
    expect(out.result.failureReason).toBe('destination_mismatch')
  })

  it('an IR with no expected destination → no_expected_destination', async () => {
    const rt = new FakeRuntime()
    const noDest = makeIr([elemStep(0, 'click', { locatorCandidates: ['#a'] })], { expectedFinalScreen: null })
    const out = await transportOk(noDest, rt)
    expect(out.result.failureReason).toBe('no_expected_destination')
  })

  it('request assertions forbid a green even when the destination matches', async () => {
    const rt = new FakeRuntime()
    const out = await transportOk(happy, rt, {
      ...makeRequest(),
      assertions: [{ kind: 'text_present', text: 'welcome' }],
    })
    expect(out.result.status).toBe('failure')
    // P5 — the final business assertion gate owns the verdict once the
    // destination matches: text_present 'welcome' fails against the live final
    // screen (title:null, headings:[]), so the green is refused as `step_failed`.
    expect(out.result.failureReason).toBe('step_failed')
  })

  // ---- Warm-benchmark finding B-1: the destination gate tolerates a BENIGN
  // `screen_ambiguous` (a same origin+path twin, e.g. an unfiltered list and the
  // same list after a client-side filter) ONLY when a P5 assertion is present to
  // confirm the business outcome, and it still defers the verdict to P5. ----
  it('B-1: benign ambiguous destination + PASSING assertion → success (warm 0-model path)', async () => {
    const rt = new FakeRuntime()
    rt.classifyHandler = (snap) =>
      snap.url === DEST_URL
        ? resolution('ambiguous', null, 'screen_ambiguous')
        : resolution('resolved', screen(42))
    const out = await executeFlow(
      { ...makeRequest(), assertions: [{ kind: 'text_present', text: 'anything' }] },
      happy,
      rt,
      {
        secrets: [],
        sleep: async (ms) => {
          rt.nowMs += ms
        },
        evaluateAssertions: (checks) =>
          checks.map((c) => ({
            kind: 'passed' as const,
            operator: c.operator.kind,
            source: 'headings' as const,
            definitive: true,
            timedOut: false,
            elapsedMs: 0,
          })),
      }
    )
    expect(out.result.status).toBe('success')
    // destination reports the LIVE url observed on the bound tab, not a stale row.
    expect(out.result.destination?.url).toBe(DEST_URL)
  })

  it('B-1: benign ambiguous destination + FAILING assertion → step_failed (never a rubber-stamp)', async () => {
    const rt = new FakeRuntime()
    rt.classifyHandler = (snap) =>
      snap.url === DEST_URL
        ? resolution('ambiguous', null, 'screen_ambiguous')
        : resolution('resolved', screen(42))
    const out = await transportOk(happy, rt, {
      ...makeRequest(),
      assertions: [{ kind: 'text_present', text: 'welcome' }],
    })
    expect(out.result.status).toBe('failure')
    expect(out.result.failureReason).toBe('step_failed')
  })

  it('B-1: ambiguous destination on a DIFFERENT path than expected → still fail-closed', async () => {
    const rt = new FakeRuntime()
    rt.classifyHandler = (snap) =>
      snap.url === DEST_URL
        ? resolution('ambiguous', null, 'screen_ambiguous')
        : resolution('resolved', screen(42))
    // expected destination is screen(99) (…/99) but the live final screen is
    // DEST_URL (…/77): origin+path diverges, so no tolerance is granted.
    const diverging = makeIr([elemStep(0, 'click', { locatorCandidates: ['#a'] })], {
      expectedFinalScreen: screen(99),
    })
    const out = await transportOk(diverging, rt, {
      ...makeRequest(),
      assertions: [{ kind: 'text_present', text: 'welcome' }],
    })
    expect(out.result.failureReason).toBe('screen_ambiguous')
  })

  // ---- Warm-benchmark finding B-3: the draft-local replay pool mints two
  // identities for pre/post content variants of ONE page (unfiltered customer
  // list vs the same list after a client-side filter — they differ only in row
  // count, hence signature) and they tie in the ambiguity band. Because the
  // executor classifies before EVERY mutation, that twin tie was failing the
  // replay closed at step 0 before the first action (measured on crm.anhtester.com).
  // When the LIVE screen's origin+path is provably an episode endpoint, the gate
  // now resolves to the start screen by URL instead of failing closed. ----
  it('B-3: replay twin ambiguity at step 0 (live on the start path) → tolerated, replay proceeds', async () => {
    const rt = new FakeRuntime()
    // START (/42) is the twin-ambiguous list; DEST (/77) resolves cleanly.
    rt.classifyHandler = (snap) =>
      snap.url === START_URL
        ? resolution('ambiguous', null, 'screen_ambiguous')
        : resolution('resolved', screen(77))
    const out = await transportOk(happy, rt)
    expect(out.result.status).toBe('success')
    expect(out.result.stepsRun).toBe(1)
  })

  it('B-3: step-0 ambiguity to a page OUTSIDE the episode endpoints → still fail-closed', async () => {
    const rt = new FakeRuntime()
    rt.classifyHandler = (snap) =>
      snap.url === START_URL
        ? resolution('ambiguous', null, 'screen_ambiguous')
        : resolution('resolved', screen(42))
    // episode endpoints are /11 and /22; the live step-0 page is /42 → outside the
    // episode, so no tolerance (a real drift still fails closed).
    const offEpisode = makeIr([elemStep(0, 'click', { locatorCandidates: ['#a'] })], {
      startScreen: screen(11),
      expectedFinalScreen: screen(22),
    })
    const out = await transportOk(offEpisode, rt)
    expect(out.result.status).toBe('ambiguous')
    expect(out.result.failureReason).toBe('screen_ambiguous')
  })

  // ---- Warm-benchmark finding B-4: a flow that CREATES a record lands on a
  // parameterized destination route with a FRESH id (/client/13846) that can
  // never equal the learned destination's exact path (/client/13845). The
  // destination gate now compares ROUTE TEMPLATES (numeric id leaf generalized),
  // still gated on ≥1 assertion so a genuinely wrong record fails the assertion.
  // A different ROUTE PREFIX (not just a new id) still fails closed. ----
  it('B-4: created-record destination on a parameterized route (new id) → tolerated via routeTemplate + assertion', async () => {
    const DETAIL_EXP = 'https://fixture.test/admin/clients/client/13845'
    const DETAIL_LIVE = 'https://fixture.test/admin/clients/client/13846'
    const rt = new FakeRuntime(baseSnapshot(START_URL), baseSnapshot(DETAIL_LIVE))
    rt.classifyHandler = (snap) =>
      snap.url === DETAIL_LIVE
        ? resolution('ambiguous', null, 'screen_ambiguous')
        : resolution('resolved', screen(42))
    const ir = makeIr([elemStep(0, 'click', { locatorCandidates: ['#a'] })], {
      expectedFinalScreen: { ...screen(100), url: DETAIL_EXP },
    })
    const out = await executeFlow(
      { ...makeRequest(), assertions: [{ kind: 'text_present', text: 'anything' }] },
      ir,
      rt,
      {
        secrets: [],
        sleep: async (ms) => {
          rt.nowMs += ms
        },
        evaluateAssertions: (checks) =>
          checks.map((c) => ({
            kind: 'passed' as const,
            operator: c.operator.kind,
            source: 'headings' as const,
            definitive: true,
            timedOut: false,
            elapsedMs: 0,
          })),
      }
    )
    expect(out.result.status).toBe('success')
    // destination reports the LIVE record url, not the stale learned one.
    expect(out.result.destination?.url).toBe(DETAIL_LIVE)
  })

  it('B-4: destination on a DIFFERENT route (not merely a new id) → still fail-closed', async () => {
    const DETAIL_EXP = 'https://fixture.test/admin/clients/client/13845'
    const OFF_ROUTE = 'https://fixture.test/admin/orders/99999'
    const rt = new FakeRuntime(baseSnapshot(START_URL), baseSnapshot(OFF_ROUTE))
    rt.classifyHandler = (snap) =>
      snap.url === OFF_ROUTE
        ? resolution('ambiguous', null, 'screen_ambiguous')
        : resolution('resolved', screen(42))
    const ir = makeIr([elemStep(0, 'click', { locatorCandidates: ['#a'] })], {
      expectedFinalScreen: { ...screen(100), url: DETAIL_EXP },
    })
    const out = await executeFlow(
      { ...makeRequest(), assertions: [{ kind: 'text_present', text: 'anything' }] },
      ir,
      rt,
      {
        secrets: [],
        sleep: async (ms) => {
          rt.nowMs += ms
        },
        evaluateAssertions: (checks) =>
          checks.map((c) => ({
            kind: 'passed' as const,
            operator: c.operator.kind,
            source: 'headings' as const,
            definitive: true,
            timedOut: false,
            elapsedMs: 0,
          })),
      }
    )
    expect(out.result.status).toBe('ambiguous')
    expect(out.result.failureReason).toBe('screen_ambiguous')
  })

  // ---- Warm-benchmark finding B-5: the SIBLING branch of B-4. Here the
  // classifier does NOT report ambiguity — it RESOLVES confidently, but to a
  // screen whose id differs from the learned destination. For a create-record
  // flow this is expected: the new record's detail page renders a distinct
  // signature (its own company name), so it elects a different id while sitting
  // on the SAME parameterized route (`/client/<newId>`). The destination gate's
  // `screenId !== expected` branch now applies the same route-template tolerance
  // as B-4 — gated on ≥1 assertion — instead of failing closed. An off-route
  // resolve, or a same-route resolve with NO assertion, still fails closed. ----
  it('B-5: resolved-to-different-id on the SAME parameterized route + assertion → tolerated (new record)', async () => {
    const DETAIL_EXP = 'https://fixture.test/admin/clients/client/13845'
    const DETAIL_LIVE = 'https://fixture.test/admin/clients/client/13846'
    const rt = new FakeRuntime(baseSnapshot(START_URL), baseSnapshot(DETAIL_LIVE))
    // Live page resolves CONFIDENTLY, but to a different id (999) than the
    // learned destination (100) — same route template, so this is a new record.
    rt.classifyHandler = (snap) =>
      snap.url === DETAIL_LIVE
        ? resolution('resolved', { ...screen(999), url: DETAIL_LIVE })
        : resolution('resolved', screen(42))
    const ir = makeIr([elemStep(0, 'click', { locatorCandidates: ['#a'] })], {
      expectedFinalScreen: { ...screen(100), url: DETAIL_EXP },
    })
    const out = await executeFlow(
      { ...makeRequest(), assertions: [{ kind: 'text_present', text: 'anything' }] },
      ir,
      rt,
      {
        secrets: [],
        sleep: async (ms) => {
          rt.nowMs += ms
        },
        evaluateAssertions: (checks) =>
          checks.map((c) => ({
            kind: 'passed' as const,
            operator: c.operator.kind,
            source: 'headings' as const,
            definitive: true,
            timedOut: false,
            elapsedMs: 0,
          })),
      }
    )
    expect(out.result.status).toBe('success')
    // Reports the LIVE record url, keeping the expected screen's identity.
    expect(out.result.destination?.url).toBe(DETAIL_LIVE)
  })

  it('B-5: resolved-to-different-id on a DIFFERENT route → still destination_mismatch', async () => {
    const DETAIL_EXP = 'https://fixture.test/admin/clients/client/13845'
    const OFF_ROUTE = 'https://fixture.test/admin/orders/99999'
    const rt = new FakeRuntime(baseSnapshot(START_URL), baseSnapshot(OFF_ROUTE))
    rt.classifyHandler = (snap) =>
      snap.url === OFF_ROUTE
        ? resolution('resolved', { ...screen(999), url: OFF_ROUTE })
        : resolution('resolved', screen(42))
    const ir = makeIr([elemStep(0, 'click', { locatorCandidates: ['#a'] })], {
      expectedFinalScreen: { ...screen(100), url: DETAIL_EXP },
    })
    const out = await executeFlow(
      { ...makeRequest(), assertions: [{ kind: 'text_present', text: 'anything' }] },
      ir,
      rt,
      {
        secrets: [],
        sleep: async (ms) => {
          rt.nowMs += ms
        },
        evaluateAssertions: (checks) =>
          checks.map((c) => ({
            kind: 'passed' as const,
            operator: c.operator.kind,
            source: 'headings' as const,
            definitive: true,
            timedOut: false,
            elapsedMs: 0,
          })),
      }
    )
    expect(out.result.status).toBe('failure')
    expect(out.result.failureReason).toBe('destination_mismatch')
  })

  it('B-5: same-route resolve but NO assertion → still destination_mismatch (guard is not a pass-through)', async () => {
    const DETAIL_EXP = 'https://fixture.test/admin/clients/client/13845'
    const DETAIL_LIVE = 'https://fixture.test/admin/clients/client/13846'
    const rt = new FakeRuntime(baseSnapshot(START_URL), baseSnapshot(DETAIL_LIVE))
    rt.classifyHandler = (snap) =>
      snap.url === DETAIL_LIVE
        ? resolution('resolved', { ...screen(999), url: DETAIL_LIVE })
        : resolution('resolved', screen(42))
    const ir = makeIr([elemStep(0, 'click', { locatorCandidates: ['#a'] })], {
      expectedFinalScreen: { ...screen(100), url: DETAIL_EXP },
    })
    // makeRequest() carries no assertions — the independent business check the
    // tolerance leans on is absent, so a differing id must fail closed.
    const out = await executeFlow(makeRequest(), ir, rt, {
      secrets: [],
      sleep: async (ms) => {
        rt.nowMs += ms
      },
    })
    expect(out.result.status).toBe('failure')
    expect(out.result.failureReason).toBe('destination_mismatch')
  })
})

// ---------------------------------------------------------------------------
// 21 — B-15: the final classification waits (bounded) for a late navigation
// commit instead of sampling a stale-but-stable page.
//
// Warm 17/20 passed while w10/w14/w18 failed as `destination_mismatch` AFTER
// 4/4 mutations: the last mutation's `waitForStable` certifies URL+title
// stability, but a save roundtrip can outlast it, so the single-shot final
// sync sampled the still-on-add page and confidently elected the wrong row.
// The convergence window below is reads-only, costs zero polls when already
// on-route, expires within the executor budget, and preserves exactly-once
// mutation accounting (no step ever re-dispatches).
// ---------------------------------------------------------------------------

describe('21 — B-15 final-route convergence waits for a late navigation commit', () => {
  const happy = makeIr([elemStep(0, 'click', { locatorCandidates: ['#a'] })])

  it('already on-route costs zero extra polls and succeeds unchanged', async () => {
    const rt = new FakeRuntime()
    const out = await run(makeRequest(), happy, rt)
    expect(out.result.status).toBe('success')
    expect(out.result.destination?.screenId).toBe(77)
    expect(rt.performLog).toHaveLength(0)
    expect(rt.dispatchLog.filter((d) => d.actionType === 'click')).toHaveLength(1)
    // Pre-step sync (1) + stabilization streak (3) + single final sync (1):
    // the convergence loop exits on its first check without sleeping.
    expect(rt.syncCalls).toBe(5)
  })

  it('a navigation that commits a few polls late still lands success (fails pre-fix)', async () => {
    const rt = new FakeRuntime()
    // waitForStable consumes exactly 3 polls certifying the stale page, then
    // the final gate sees 2 more stale polls before the commit lands. A
    // single-shot final sync (pre-fix) samples poll #5 and fails closed.
    rt.postLandingQueue = Array.from({ length: 5 }, () => baseSnapshot(START_URL))
    const out = await run(makeRequest(), happy, rt)
    expect(out.result.status).toBe('success')
    expect(out.result.destination?.screenId).toBe(77)
    // Exactly-once: the step dispatched once; only reads were repeated.
    expect(rt.dispatchLog.filter((d) => d.actionType === 'click')).toHaveLength(1)
  })

  it('a navigation that never commits still fails closed as destination_mismatch, bounded', async () => {
    const rt = new FakeRuntime()
    rt.neverNavigate = true
    const before = rt.nowMs
    const out = await run(makeRequest(), happy, rt)
    expect(out.result.status).toBe('failure')
    expect(out.result.failureReason).toBe('destination_mismatch')
    expect(out.result.stepsRun).toBe(1)
    // Exactly-once: the step dispatched once; the convergence window only
    // re-read and then gave up.
    expect(rt.dispatchLog.filter((d) => d.actionType === 'click')).toHaveLength(1)
    // Bounded: stabilization (~3 polls) + convergence capped by the 3 s
    // standard budget at 250 ms polls — far below any executor timeout.
    expect(rt.syncCalls).toBeLessThan(30)
    expect(out.result.metrics.wallDurationMs).toBeLessThan(10_000)
    expect(rt.nowMs - before).toBeLessThan(10_000)
  })
})

// =============================================================================
// LIVE CONTRACT — readRefValue envelope normalization.
// The installed-extension gate proved the write ON THE BROWSER SIDE
// (`result.verified:true` over the wire) while core still reported
// "readRefValue probe unconfirmed — write not verified": the interpreter only
// understood the pre-router page shape `{ok:true, result:{value:{ok}}}` while
// the shipped router flattens it to
// `{ok:true, result:{refId, verified, match}}`
// (packages/chrome-extension/background.js). This block pins both shapes, the
// fail-closed boundary, and the producer's source itself — the two contracts
// cannot drift apart again silently.
// =============================================================================
describe('readRefValue live contract — identity-bound probe envelope', () => {
  const SECRET = 'hunter2-canary-value'

  it('accepts the live extension router envelope as the write proof', () => {
    // Byte-for-byte the object the extension's own router test asserts.
    expect(
      interpretSemanticRefValueProbe({
        ok: true,
        result: { refId: 'dom:1:2', verified: true, match: 'equals' },
      })
    ).toBe('ok')
    expect(
      interpretSemanticRefValueProbe({
        ok: true,
        result: { refId: 'dom:9:9', verified: true, match: 'contains' },
      })
    ).toBe('ok')
  })

  it('reads router verified:false as a definite mismatch', () => {
    expect(
      interpretSemanticRefValueProbe({
        ok: true,
        result: { refId: 'dom:1:2', verified: false, match: 'equals' },
      })
    ).toBe('mismatch')
  })

  it('still accepts the legacy pre-router page envelope (true and false)', () => {
    expect(
      interpretSemanticRefValueProbe({ ok: true, result: { value: { ok: true } } })
    ).toBe('ok')
    expect(
      interpretSemanticRefValueProbe({
        ok: true,
        result: { value: { ok: false, code: 'VALUE_MISMATCH' } },
      })
    ).toBe('mismatch')
  })

  it('refuses a transport-level ok carrying no boolean page verdict', () => {
    // The old loose-shape trap: `{ok:true, result:{ok:true}}` says the
    // DISPATCH worked — nothing about the value on the node.
    expect(interpretSemanticRefValueProbe({ ok: true, result: { ok: true } })).toBe(
      'unverifiable'
    )
    expect(interpretSemanticRefValueProbe({ ok: true, result: {} })).toBe('unverifiable')
    expect(interpretSemanticRefValueProbe({ ok: true })).toBe('unverifiable')
    expect(interpretSemanticRefValueProbe({ ok: true, result: null })).toBe('unverifiable')
    expect(interpretSemanticRefValueProbe({ ok: true, result: 'laptop' })).toBe('unverifiable')
  })

  it('refuses transport rejections (ok !== true) regardless of payload', () => {
    expect(
      interpretSemanticRefValueProbe({
        ok: false,
        error: { code: 'REF_NOT_FOUND', message: 'ref not in cache' },
      })
    ).toBe('unverifiable')
    expect(
      interpretSemanticRefValueProbe({
        ok: false,
        result: { refId: 'dom:1:2', verified: true, match: 'equals' },
      })
    ).toBe('unverifiable')
    // Control: the same result under a real transport ok DOES decide.
    expect(
      interpretSemanticRefValueProbe({ ok: true, result: { verified: true, match: 'equals' } })
    ).toBe('ok')
    expect(
      interpretSemanticRefValueProbe({ ok: 'true', result: { verified: true } })
    ).toBe('unverifiable')
    expect(interpretSemanticRefValueProbe({ result: { verified: true } })).toBe('unverifiable')
  })

  it('refuses non-boolean verdicts — a raw value wearing the verdict slot decides nothing', () => {
    expect(interpretSemanticRefValueProbe({ ok: true, result: { verified: 'true' } })).toBe(
      'unverifiable'
    )
    expect(interpretSemanticRefValueProbe({ ok: true, result: { verified: 1 } })).toBe(
      'unverifiable'
    )
    expect(interpretSemanticRefValueProbe({ ok: true, result: { verified: SECRET } })).toBe(
      'unverifiable'
    )
    expect(interpretSemanticRefValueProbe({ ok: true, result: { value: SECRET } })).toBe(
      'unverifiable'
    )
    expect(
      interpretSemanticRefValueProbe({ ok: true, result: { value: { ok: 'true' } } })
    ).toBe('unverifiable')
  })

  it('refuses conflicting dual envelopes; accepts only agreeing ones', () => {
    expect(
      interpretSemanticRefValueProbe({
        ok: true,
        result: { verified: true, value: { ok: false, code: 'VALUE_MISMATCH' } },
      })
    ).toBe('unverifiable')
    expect(
      interpretSemanticRefValueProbe({
        ok: true,
        result: { verified: false, value: { ok: true } },
      })
    ).toBe('unverifiable')
    expect(
      interpretSemanticRefValueProbe({ ok: true, result: { verified: true, value: { ok: true } } })
    ).toBe('ok')
    expect(
      interpretSemanticRefValueProbe({
        ok: true,
        result: { verified: false, value: { ok: false } },
      })
    ).toBe('mismatch')
  })

  it('refuses malformed raw envelopes of every shape', () => {
    for (const raw of [
      null,
      undefined,
      0,
      42,
      '',
      'verified',
      true,
      false,
      [],
      ['verified'],
      [{ verified: true }],
    ]) {
      expect(interpretSemanticRefValueProbe(raw)).toBe('unverifiable')
    }
  })

  it('never echoes a raw field value through its verdict', () => {
    // The function can only return the three literals — whatever arrives.
    for (const raw of [
      { ok: true, result: { refId: 'dom:1:2', verified: true, value: SECRET } },
      { ok: true, result: { verified: SECRET } },
      { ok: true, value: SECRET },
      { ok: false, result: { error: SECRET } },
    ]) {
      const out = interpretSemanticRefValueProbe(raw)
      expect(['ok', 'mismatch', 'unverifiable']).toContain(out)
      expect(String(out)).not.toContain(SECRET)
    }
  })

  it('contract lock: the shipped extension router emits exactly the canonical envelope', () => {
    // Producer side parsed straight out of the extension source. If anyone
    // renames or drops `verified` on the router result, this fails BEFORE the
    // next live gate can rediscover the mismatch by hand.
    const bg = fs.readFileSync(
      path.resolve(__dirname, '..', '..', '..', 'packages', 'chrome-extension', 'background.js'),
      'utf8'
    )
    const caseStart = bg.indexOf('if (type === "readRefValue")')
    expect(caseStart).toBeGreaterThan(-1)
    const rest = bg.slice(caseStart)
    const caseEnd = rest.indexOf('if (type === "clearRefCache")')
    expect(caseEnd).toBeGreaterThan(-1)
    const routerCase = rest.slice(0, caseEnd)
    // Success envelope: transport ok:true with a result object…
    expect(routerCase).toMatch(/return\s*\{\s*ok:\s*true,\s*result:\s*\{/)
    // …carrying a boolean `verified` derived from the page op's value verdict…
    expect(routerCase).toMatch(/verified:\s*[^\n]*===\s*true\s*,/)
    // …alongside the echoed refId and the match mode.
    expect(routerCase).toMatch(/refId,/)
    expect(routerCase).toMatch(/match,?\s*\}/)
    // Transport failures answer ok:false with a typed error (never a verdict).
    expect(routerCase).toMatch(/ok:\s*false,\s*error:\s*\{\s*code:\s*"REF_NOT_FOUND"/)
    // And the core interpreter accepts the canonical literal unchanged —
    // the same object the extension suite asserts with toEqual.
    expect(
      interpretSemanticRefValueProbe({
        ok: true,
        result: { refId: 'dom:1:1', verified: true, match: 'equals' },
      })
    ).toBe('ok')
    expect(
      interpretSemanticRefValueProbe({
        ok: true,
        result: { refId: 'dom:1:1', verified: false, match: 'equals' },
      })
    ).toBe('mismatch')
  })
})

// ---------------------------------------------------------------------------
// 13. B-8 — a slot-template recorded name anchors the semantic ref across
// list-order drift; the positional ref stays the fallback.
// ---------------------------------------------------------------------------

describe('13 — B-8 slot-template name anchoring', () => {
  const rows = (targetName: string): LiveRefNodeV01[] => [
    { refId: 'ref-add', role: 'button', name: 'Create new organisation' },
    { refId: 'ref-old', role: 'button', name: 'Open DRIFT-e7d3 — d.e7d3@fixture.test' },
    { refId: 'ref-mine', role: 'button', name: targetName },
  ]

  const rowStep = (ref: string, name: string): FlowIRStepV01 =>
    elemStep(0, 'click', { role: 'button', ref, name })

  const data = {
    company: 'DRIFT-e7d4',
    email: 'd.e7d4@fixture.test',
    phone: '09000000',
  }

  it('clicks the name-matched node when the recorded index points at a stale row', async () => {
    // Learned when the episode's row sat at position 1; this run's list grew —
    // `button@1` is now ANOTHER customer's row. Positional binding would open
    // the wrong business object (the fixture measured exactly this).
    const ir = makeIr([rowStep('button@1', 'Open $company — $email')])
    const rt = new FakeRuntime()
    rt.refNodes = rows('Open DRIFT-e7d4 — d.e7d4@fixture.test')
    const out = await run(makeRequest(data), ir, rt)
    expect(out.result.status).toBe('success')
    expect(rt.performLog).toHaveLength(1)
    expect(rt.performLog[0]).toMatchObject({ refId: 'ref-mine', method: 'click' })
  })

  it('renders through the click for a correct index too (idempotent override)', async () => {
    const ir = makeIr([rowStep('button@2', 'Open $company — $email')])
    const rt = new FakeRuntime()
    rt.refNodes = rows('Open DRIFT-e7d4 — d.e7d4@fixture.test')
    const out = await run(makeRequest(data), ir, rt)
    expect(out.result.status).toBe('success')
    expect(rt.performLog[0].refId).toBe('ref-mine')
  })

  it('an ambiguous rendered name keeps the positional binding — a shared name proves nothing', async () => {
    const ir = makeIr([rowStep('button@1', 'Open $company — $email')])
    const rt = new FakeRuntime()
    // Two rows render to the same name (duplicate companies): no anchor.
    rt.refNodes = [
      { refId: 'ref-a', role: 'button', name: 'Open DRIFT-e7d4 — d.e7d4@fixture.test' },
      { refId: 'ref-b', role: 'button', name: 'Open DRIFT-e7d4 — d.e7d4@fixture.test' },
    ]
    const out = await run(makeRequest(data), ir, rt)
    expect(out.result.status).toBe('success')
    expect(rt.performLog[0].refId).toBe('ref-b') // index 1 → second button
  })

  it('an unresolvable token never shadows the positional binding', async () => {
    const ir = makeIr([rowStep('button@1', 'Open $company — $missing_slot')])
    const rt = new FakeRuntime()
    rt.refNodes = rows('Open DRIFT-e7d4 — d.e7d4@fixture.test')
    const out = await run(makeRequest(data), ir, rt)
    expect(out.result.status).toBe('success')
    expect(rt.performLog[0].refId).toBe('ref-old') // positional, pre-B-8 behavior
  })

  it('a literal recorded name never overrides the position', async () => {
    const ir = makeIr([rowStep('button@1', 'Open DRIFT-e7d4 — d.e7d4@fixture.test')])
    const rt = new FakeRuntime()
    rt.refNodes = rows('Open DRIFT-e7d4 — d.e7d4@fixture.test')
    const out = await run(makeRequest(data), ir, rt)
    expect(out.result.status).toBe('success')
    expect(rt.performLog[0].refId).toBe('ref-old') // no $tokens: positional stands
  })

  it('whitespace differences do not defeat the anchor', async () => {
    const ir = makeIr([rowStep('button@1', 'Open $company — $email')])
    const rt = new FakeRuntime()
    rt.refNodes = rows('Open   DRIFT-e7d4  —  d.e7d4@fixture.test')
    const out = await run(makeRequest(data), ir, rt)
    expect(out.result.status).toBe('success')
    expect(rt.performLog[0].refId).toBe('ref-mine')
  })

  it('helper: finds nothing when no node carries a name at all', () => {
    expect(
      nameAnchoredLiveRef(
        [{ refId: 'x', role: 'button' }],
        'button',
        'click',
        'Open $company',
        { company: 'C' }
      )
    ).toBeNull()
  })
})

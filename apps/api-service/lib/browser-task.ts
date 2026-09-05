/**
 * Browser task high-level surface (P1, extended with the P4 async seam).
 *
 * One model-facing entry point: a model submits intent + named data +
 * assertions; the core returns ONE terminal `BrowserTaskResultV01`. There is
 * no internal/external LLM retry loop — one multi-step task submission is at
 * most one outer model tool call.
 *
 * P1 scope: schema + size validation, required-data validation before any
 * browser mutation, redaction of secret data values, terminal structured
 * results, canonical failure reasons only, and a fast / deterministic /
 * non-mutating "compiler not available" path. P3 owns compilation, P4 owns
 * execution. This module never touches a browser, the planner, the executor,
 * the cache, discovery, or session ownership.
 *
 * P4 adds an ASYNC execution seam on top of the frozen synchronous surface:
 *   - `BrowserTaskDeps.execute` + `runBrowserTaskAsync` /
 *     `runValidatedBrowserTaskAsync` let a runner consume a compiled IR
 *     asynchronously (P4's browser-task-runner.ts does exactly this).
 *   - The synchronous `runBrowserTask` / `runValidatedBrowserTask` behaviors
 *     are preserved byte-for-byte (a compiled IR still returns the terminal
 *     `error` + executor-gap note).
 * The shared validation is extracted into `prepareBrowserTask` /
 * `compileValidatedRequest` so the sync and async paths provably run the same
 * checks in the same order.
 */
import {
  BROWSER_TASK_VERSION_LITERAL,
  missingRequiredDataSlots,
  validateBrowserTaskRequest,
} from './browser-task-schema'
import { collectSecretValues, redactText } from './redaction'
import type {
  BrowserFlowIRV01,
  BrowserTaskFailureReasonV01,
  BrowserTaskRequestV01,
  BrowserTaskResultV01,
} from '../types/browser-task'

/** Compile outcome produced by the P3 task compiler seam. */
export type BrowserTaskCompileOutcome =
  | { ok: true; ir: BrowserFlowIRV01 }
  | { ok: false; reason: Exclude<BrowserTaskFailureReasonV01, 'error'> }

/**
 * Runner-owned seams. P1 keeps the defaults (no compiler → `no_scenario`); tests
 * inject stubs to prove ordering and the non-mutating failure paths. P3 fills
 * `compile`; P4 fills `execute`. Selection of browser target / Profile 30
 * affinity / discovery / auth / planner policy stays runner-owned — the model
 * never declares selectors, leases, profiles, auth mode, cache tiers, discovery
 * policy, planner policy, browser backend, or tab choice.
 */
export interface BrowserTaskDeps {
  requiredDataSlots?: (request: BrowserTaskRequestV01) => string[]
  compile?: (request: BrowserTaskRequestV01) => BrowserTaskCompileOutcome
  /** P4 async execution seam. Absent → the executor-gap terminal. */
  execute?: (
    request: BrowserTaskRequestV01,
    ir: BrowserFlowIRV01,
    ctx: ExecutorExecuteContextV01
  ) => Promise<BrowserTaskOutcome>
}

/** Clock + secret context handed to the P4 `execute` seam. */
export interface ExecutorExecuteContextV01 {
  now: () => number
  startedAt: number
  /** Secret data values to scrub from any surfaced detail. */
  secrets: string[]
}

export interface BrowserTaskRunOptions {
  deps?: BrowserTaskDeps
  /** Injectable clock for deterministic tests. */
  now?: () => number
}

/**
 * Outcome of one task run. `result` is always the frozen §3 terminal result;
 * `note` is a P1 transport-level, human-readable detail that is never part of
 * the frozen contract. The MCP tool surfaces `note` as `errorText`.
 */
export interface BrowserTaskVerification {
  status: 'passed' | 'failed' | 'not_run'
  assertionsPassed: number
  assertionsTotal: number
}

export interface BrowserTaskOutcome {
  result: BrowserTaskResultV01
  /** Set only by the real executor after evaluating the request assertions. */
  verification?: BrowserTaskVerification
  note?: string
}

/** Compiler gap message — explicit, and never a fake success. */
const COMPILER_NOT_AVAILABLE_MESSAGE =
  'browser-task compilation is not implemented until P3 — no scenario could be ' +
  'compiled for this request. Nothing was executed and nothing was mutated.'

/** Executor gap message — a compiled IR cannot run without an executor seam. */
const EXECUTOR_NOT_AVAILABLE_MESSAGE =
  'browser-task execution is not implemented until P4 — no executor seam was ' +
  'provided for the compiled IR. Nothing was executed and nothing was mutated.'

/**
 * Terminal outcome for a throwing runner-owned seam. The runner-owned deps
 * (`requiredDataSlots`, `compile`, `execute`) must NEVER be able to make this
 * surface throw — every result is terminal. The exception detail is capped and
 * scrubbed of known secret values before it is surfaced in the note.
 */
function seamFailureOutcome(
  label: string,
  err: unknown,
  requestId: string,
  startedAt: number,
  now: () => number,
  secrets: string[]
): BrowserTaskOutcome {
  const raw = err instanceof Error ? err.message : String(err)
  const detail = redactText(raw.slice(0, 300) || 'unknown error', secrets)
  return {
    result: terminalResult(
      { requestId, status: 'error', failureReason: 'error' },
      startedAt,
      now
    ),
    note: redactText(
      `runner-owned ${label} seam threw (${detail}) — terminal error; ` +
        'nothing was executed and nothing was mutated',
      secrets
    ),
  }
}

/** Build a terminal (0 steps) frozen §3 result. Exported for the P4 runner. */
export function terminalResult(
  opts: {
    requestId: string
    status: BrowserTaskResultV01['status']
    failureReason?: BrowserTaskFailureReasonV01
  },
  startedAt: number,
  now: () => number
): BrowserTaskResultV01 {
  const elapsedMs = Math.max(0, now() - startedAt)
  return {
    version: BROWSER_TASK_VERSION_LITERAL,
    requestId: opts.requestId,
    status: opts.status,
    stepsRun: 0,
    stepsTotal: 0,
    ...(opts.failureReason ? { failureReason: opts.failureReason } : {}),
    metrics: {
      coreDurationMs: elapsedMs,
      wallDurationMs: elapsedMs,
      modelApiCalls: 0,
      cacheHits: 0,
      path: [],
    },
  }
}

/** Map a canonical failure reason onto the frozen §3 status. Exported for the P4 runner. */
export function statusForReason(
  reason: BrowserTaskFailureReasonV01
): BrowserTaskResultV01['status'] {
  switch (reason) {
    case 'screen_ambiguous':
      return 'ambiguous'
    case 'contract_conflict':
      return 'contract_conflict'
    case 'no_extension':
    case 'extension_timeout':
    case 'scope_denied':
    case 'daemon_unreachable':
    case 'error':
      return 'error'
    default:
      return 'failure'
  }
}

function bestEffortRequestId(rawArgs: unknown): string {
  const asObject = coerceToObject(rawArgs)
  if (asObject) {
    const id = (asObject as { requestId?: unknown }).requestId
    if (typeof id === 'string') return id
  }
  return ''
}

/** JSON-string args arrive as strings at the MCP boundary; normalize to object. */
function coerceToObject(rawArgs: unknown): unknown {
  if (typeof rawArgs === 'string') {
    try {
      return JSON.parse(rawArgs) as unknown
    } catch {
      return null
    }
  }
  return rawArgs
}

/**
 * The P1 default compiler outcome: the P3 task compiler is not implemented, so
 * every submitted task fails explicitly and deterministically with the canonical
 * `no_scenario` reason — fast and non-mutating by construction.
 */
export function defaultCompile(_request: BrowserTaskRequestV01): BrowserTaskCompileOutcome {
  return { ok: false, reason: 'no_scenario' }
}

/**
 * Shared pre-compile validation for a raw call argument: coerce → collect secret
 * values → version gate → schema validation. Returns either a terminal outcome
 * (version conflict / schema error) or a validated request plus its secret set
 * and clock, ready for the runner-owned `requiredDataSlots` + `compile` + P4
 * `execute` seams. Every entry point — sync, async, and the P4 production
 * runner — passes through this exact sequence, so a version or schema problem is
 * rejected identically on every path.
 */
export function prepareBrowserTask(
  rawArgs: unknown,
  options: BrowserTaskRunOptions = {}
):
  | { kind: 'terminal'; outcome: BrowserTaskOutcome }
  | { kind: 'ready'; request: BrowserTaskRequestV01; secrets: string[]; now: () => number; startedAt: number } {
  const now = options.now ?? Date.now
  const startedAt = now()

  // Secret-hygiene from the very first exit: every data value is treated as
  // sensitive, so even pre-validation notes (version mismatch, schema issues)
  // are scrubbed before they can surface. collectSecretValues only reads `data`
  // values, which is safe on a possibly-unvalidated object.
  const asObject = coerceToObject(rawArgs)
  const secrets = collectSecretValues(
    asObject && typeof asObject === 'object' && !Array.isArray(asObject)
      ? (asObject as Pick<BrowserTaskRequestV01, 'data'>)
      : undefined
  )

  // Version gate runs before full schema validation so an unsupported version
  // surfaces as the distinct terminal `contract_conflict` status, not a generic
  // schema error.
  const version =
    asObject && typeof asObject === 'object' && !Array.isArray(asObject)
      ? (asObject as { version?: unknown }).version
      : undefined
  if (version !== undefined && version !== BROWSER_TASK_VERSION_LITERAL) {
    return {
      kind: 'terminal',
      outcome: {
        result: terminalResult(
          {
            requestId: bestEffortRequestId(rawArgs),
            status: 'contract_conflict',
            failureReason: 'contract_conflict',
          },
          startedAt,
          now
        ),
        note: redactText(
          `unsupported version ${String(version)} — this surface only accepts ${BROWSER_TASK_VERSION_LITERAL}`,
          secrets
        ),
      },
    }
  }

  const validation = validateBrowserTaskRequest(rawArgs)
  if (!validation.ok) {
    return {
      kind: 'terminal',
      outcome: {
        result: terminalResult(
          {
            requestId: bestEffortRequestId(rawArgs),
            status: 'error',
            failureReason: 'error',
          },
          startedAt,
          now
        ),
        note: redactText(
          `invalid browserTask request: ${validation.issues.join('; ')}`,
          secrets
        ),
      },
    }
  }

  return { kind: 'ready', request: validation.request, secrets, now, startedAt }
}

/**
 * Shared runner-owned-seam stage for a validated request: version gate →
 * `requiredDataSlots` → `compile`. Returns either a terminal outcome (version
 * conflict / missing required data / compile failure) or the compiled IR ready
 * for the P4 `execute` seam. The seam order is fixed and provably precedes any
 * browser access.
 */
function compileValidatedRequest(
  request: BrowserTaskRequestV01,
  deps: BrowserTaskDeps,
  now: () => number,
  startedAt: number,
  secrets: string[]
):
  | { kind: 'terminal'; outcome: BrowserTaskOutcome }
  | { kind: 'compiled'; ir: BrowserFlowIRV01 } {
  if (request.version !== BROWSER_TASK_VERSION_LITERAL) {
    return {
      kind: 'terminal',
      outcome: {
        result: terminalResult(
          { requestId: request.requestId, status: 'contract_conflict', failureReason: 'contract_conflict' },
          startedAt,
          now
        ),
        note: redactText(
          `unsupported version ${request.version} — only ${BROWSER_TASK_VERSION_LITERAL} is accepted`,
          secrets
        ),
      },
    }
  }

  // Validate ALL required named data BEFORE any browser mutation. With no
  // compiler yet the default contract declares no required slots, but the
  // ordering is fixed: a missing required slot stops the task before `compile`
  // is even consulted — provably before any browser access. The seam is
  // runner-owned and wrapped: a throw becomes a terminal `error` result, never
  // a thrown exception.
  let required: string[]
  try {
    required = deps.requiredDataSlots ? deps.requiredDataSlots(request) : []
  } catch (err) {
    return {
      kind: 'terminal',
      outcome: seamFailureOutcome('requiredDataSlots', err, request.requestId, startedAt, now, secrets),
    }
  }
  const missing = missingRequiredDataSlots(request, required)
  if (missing.length > 0) {
    return {
      kind: 'terminal',
      outcome: {
        result: terminalResult(
          { requestId: request.requestId, status: statusForReason('missing_data'), failureReason: 'missing_data' },
          startedAt,
          now
        ),
        note: redactText(
          `missing required named data: ${missing.join(', ')} — validated before any browser mutation; nothing was executed`,
          secrets
        ),
      },
    }
  }

  // Compile seam. P1: the compiler does not exist → explicit, fast,
  // deterministic, non-mutating failure. Stubs may return other canonical
  // reasons (screen_ambiguous, no_path, ...) which map to their frozen status.
  // The seam is runner-owned and wrapped: a throwing compiler (P3+) must not be
  // able to escape a non-terminal exception.
  let compileOutcome: BrowserTaskCompileOutcome
  if (deps.compile) {
    try {
      compileOutcome = deps.compile(request)
    } catch (err) {
      return {
        kind: 'terminal',
        outcome: seamFailureOutcome('compile', err, request.requestId, startedAt, now, secrets),
      }
    }
  } else {
    compileOutcome = defaultCompile(request)
  }
  if (!compileOutcome.ok) {
    const reason = compileOutcome.reason
    const message =
      reason === 'no_scenario'
        ? COMPILER_NOT_AVAILABLE_MESSAGE
        : `task could not be compiled: ${reason}`
    return {
      kind: 'terminal',
      outcome: {
        result: terminalResult(
          { requestId: request.requestId, status: statusForReason(reason), failureReason: reason },
          startedAt,
          now
        ),
        note: redactText(`${message} (${reason})`, secrets),
      },
    }
  }

  return { kind: 'compiled', ir: compileOutcome.ir }
}

/**
 * Run one browser task from a raw call argument. ALWAYS returns a terminal
 * outcome — never throws, never yields a partial result.
 */
export function runBrowserTask(
  rawArgs: unknown,
  options: BrowserTaskRunOptions = {}
): BrowserTaskOutcome {
  const prepared = prepareBrowserTask(rawArgs, options)
  if (prepared.kind === 'terminal') return prepared.outcome
  return runValidatedBrowserTask(prepared.request, { ...options, now: prepared.now })
}

/**
 * Run a task whose request has already passed schema + size validation.
 * Validates ALL required named data before any browser mutation.
 */
export function runValidatedBrowserTask(
  request: BrowserTaskRequestV01,
  options: BrowserTaskRunOptions = {}
): BrowserTaskOutcome {
  const now = options.now ?? Date.now
  const startedAt = now()
  const deps = options.deps ?? {}

  // Secret-hygiene: every data value is secret. Scrub any note against the real
  // secret values before it can surface to the caller / a trace.
  const secrets = collectSecretValues(request)

  const compiled = compileValidatedRequest(request, deps, now, startedAt, secrets)
  if (compiled.kind === 'terminal') return compiled.outcome

  // A compiled IR is reachable today only via a test stub; P4 execution needs
  // the async `execute` seam (`runBrowserTaskAsync` / the P4 production runner).
  // Never pretend success — the synchronous surface has no executor.
  return {
    result: terminalResult(
      { requestId: request.requestId, status: 'error', failureReason: 'error' },
      startedAt,
      now
    ),
    note: redactText(EXECUTOR_NOT_AVAILABLE_MESSAGE, secrets),
  }
}

/**
 * P4 async entry point. Identical validation order to `runBrowserTask`, then —
 * when the request compiles — awaits the runner-owned `execute` seam. Absent an
 * `execute` seam, returns the same executor-gap terminal as the sync surface.
 * ALWAYS resolves to a terminal outcome; never rejects, never yields a partial
 * result.
 */
export async function runBrowserTaskAsync(
  rawArgs: unknown,
  options: BrowserTaskRunOptions = {}
): Promise<BrowserTaskOutcome> {
  const prepared = prepareBrowserTask(rawArgs, options)
  if (prepared.kind === 'terminal') return prepared.outcome
  return runValidatedBrowserTaskAsync(prepared.request, { ...options, now: prepared.now })
}

/**
 * P4 async variant of `runValidatedBrowserTask`. Same runner-owned seam order,
 * then dispatches a compiled IR to `deps.execute`. A throwing `execute` seam is
 * converted to a terminal `error` outcome — never a rejected promise.
 */
export async function runValidatedBrowserTaskAsync(
  request: BrowserTaskRequestV01,
  options: BrowserTaskRunOptions = {}
): Promise<BrowserTaskOutcome> {
  const now = options.now ?? Date.now
  const startedAt = now()
  const deps = options.deps ?? {}
  const secrets = collectSecretValues(request)

  const compiled = compileValidatedRequest(request, deps, now, startedAt, secrets)
  if (compiled.kind === 'terminal') return compiled.outcome

  if (deps.execute) {
    try {
      const outcome = await deps.execute(request, compiled.ir, { now, startedAt, secrets })
      return outcome
    } catch (err) {
      return seamFailureOutcome('execute', err, request.requestId, startedAt, now, secrets)
    }
  }

  return {
    result: terminalResult(
      { requestId: request.requestId, status: 'error', failureReason: 'error' },
      startedAt,
      now
    ),
    note: redactText(EXECUTOR_NOT_AVAILABLE_MESSAGE, secrets),
  }
}

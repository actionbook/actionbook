/**
 * Secret data-value redaction (P1 task 4).
 *
 * P0 contract (`.docs/browser-task-core/p0-contracts.md` §2): sensitive values
 * (passwords, tokens, cookies) may only ever travel inside `request.data`
 * values — never in `goal`, assertions, prompts, or IR. Every `data` value is
 * therefore treated as sensitive and redacted from logs, traces, errors, and
 * authoring output. Secrets never appear in any persisted artifact.
 */
import type { BrowserTaskRequestV01 } from '../types/browser-task'

/** Placeholder replacing every redacted data value. */
export const REDACTION_PLACEHOLDER = '[REDACTED]'

export type RedactedDataValue = string

/** Redact one primitive data value. Values are primitives; all are treated as sensitive. */
export function redactValue(value: string | number | boolean): RedactedDataValue {
  return REDACTION_PLACEHOLDER
}

/**
 * Redact every value of a named-data record. Keys are preserved (they are
 * slot names like `email` / `password` — names, not values); values are
 * replaced by `REDACTION_PLACEHOLDER`.
 */
export function redactDataValues(
  data?: Record<string, string | number | boolean>
): Record<string, RedactedDataValue> {
  if (!data) return {}
  const out: Record<string, RedactedDataValue> = {}
  for (const key of Object.keys(data)) {
    out[key] = REDACTION_PLACEHOLDER
  }
  return out
}

/**
 * The actual secret strings carried by a request's `data` values (as strings).
 * Used to scrub the same values out of free-form text (error strings, traces).
 */
export function collectSecretValues(
  request?: Pick<BrowserTaskRequestV01, 'data'>
): string[] {
  if (!request?.data) return []
  const secrets: string[] = []
  for (const value of Object.values(request.data)) {
    if (typeof value === 'string' && value.length > 0) {
      secrets.push(value)
    }
  }
  return secrets
}

/**
 * Replace every occurrence of the given secret values inside `text` with the
 * redaction placeholder. Guards against a secret leaking through an error /
 * trace / log line that concatenated a data value.
 */
export function redactText(text: string, secrets: string[]): string {
  let out = text
  for (const secret of secrets) {
    if (secret.length === 0) continue
    // Escape once per call; regex-special characters inside secrets are literal.
    const escaped = secret.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
    out = out.split(new RegExp(escaped, 'g')).join(REDACTION_PLACEHOLDER)
  }
  return out
}

/**
 * A log-safe view of a request. Every `data` value is replaced by
 * `REDACTION_PLACEHOLDER`, and any known secret value (any string carried in
 * `data`) is scrubbed out of free-form text too — `goal`, `startUrl`, and every
 * assertion string field. A data value echoed verbatim into a goal or assertion
 * must never survive in a log / trace / artifact. IR / prompt text is never
 * emitted.
 */
export interface RedactedRequestView {
  version: BrowserTaskRequestV01['version']
  requestId: string
  goal: string
  startUrl?: string
  data: Record<string, RedactedDataValue>
  assertions?: BrowserTaskRequestV01['assertions']
  timeoutMs?: number
}

/**
 * Scrub known secret values out of every free-text field of an assertion.
 * A secret may never surface through an assertion even if the writer echoed it
 * there (P0 §2: secrets travel only inside `data`).
 */
function redactAssertions(
  assertions: BrowserTaskRequestV01['assertions'],
  secrets: string[]
): BrowserTaskRequestV01['assertions'] {
  if (!assertions) return assertions
  return assertions.map((a) => {
    switch (a.kind) {
      case 'text_present':
        return { ...a, text: redactText(a.text, secrets) }
      case 'url_pattern':
        return { ...a, pattern: redactText(a.pattern, secrets) }
      case 'element_state':
        return { ...a, scope: redactText(a.scope, secrets) }
      case 'count':
        return { ...a, selectorScope: redactText(a.selectorScope, secrets) }
      case 'custom_expression':
        return { ...a, expression: redactText(a.expression, secrets) }
    }
  })
}

export function redactRequestForLog(
  request: BrowserTaskRequestV01
): RedactedRequestView {
  const secrets = collectSecretValues(request)
  return {
    version: request.version,
    requestId: request.requestId,
    goal: redactText(request.goal, secrets),
    startUrl: request.startUrl ? redactText(request.startUrl, secrets) : undefined,
    data: redactDataValues(request.data),
    assertions: redactAssertions(request.assertions, secrets),
    timeoutMs: request.timeoutMs,
  }
}
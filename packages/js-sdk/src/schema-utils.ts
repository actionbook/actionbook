import { z } from 'zod'
import { zodToJsonSchema } from 'zod-to-json-schema'

/**
 * Convert a Zod schema to JSON Schema format suitable for LLM tool definitions.
 *
 * This function:
 * - Converts to OpenAPI 3 compatible JSON Schema
 * - Removes $schema, default, and additionalProperties fields
 * - Fixes exclusiveMinimum/exclusiveMaximum for JSON Schema draft 2020-12 compatibility
 *
 * @example
 * ```typescript
 * const schema = z.object({
 *   query: z.string().min(1).max(200),
 *   limit: z.number().int().min(1).max(100).optional(),
 * })
 *
 * const jsonSchema = toolInputToJsonSchema(schema)
 * // Can be used with OpenAI, Anthropic, Gemini, etc.
 * ```
 */
export function toolInputToJsonSchema(schema: z.ZodType): object {
  const jsonSchema = zodToJsonSchema(schema, {
    $refStrategy: 'none',
    target: 'openApi3',
  }) as Record<string, unknown>

  // Drop top-level $schema to align with MCP/LLM expectations
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  const { $schema, ...rest } = jsonSchema

  // Remove 'default' from properties as Claude API may not support it
  const cleaned = removeDefaults(rest)
  return cleaned
}

function removeDefaults(obj: any): any {
  if (typeof obj !== 'object' || obj === null) {
    return obj
  }

  if (Array.isArray(obj)) {
    return obj.map(removeDefaults)
  }

  const result: any = {}
  for (const [key, value] of Object.entries(obj)) {
    // Skip problematic fields for Claude API
    if (key === 'default' || key === 'additionalProperties') {
      continue
    }

    // Fix exclusiveMinimum/exclusiveMaximum for JSON Schema draft 2020-12
    if (
      key === 'exclusiveMinimum' &&
      value === true &&
      obj.minimum !== undefined
    ) {
      // Old format: { exclusiveMinimum: true, minimum: 0 }
      // New format: { exclusiveMinimum: 0 }
      result.exclusiveMinimum = obj.minimum
      continue
    }
    if (
      key === 'exclusiveMaximum' &&
      value === true &&
      obj.maximum !== undefined
    ) {
      result.exclusiveMaximum = obj.maximum
      continue
    }
    if (key === 'minimum' && obj.exclusiveMinimum === true) {
      // Skip minimum when we have exclusiveMinimum
      continue
    }
    if (key === 'maximum' && obj.exclusiveMaximum === true) {
      // Skip maximum when we have exclusiveMaximum
      continue
    }

    result[key] = removeDefaults(value)
  }
  return result
}

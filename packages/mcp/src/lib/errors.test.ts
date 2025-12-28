import { describe, expect, it } from 'vitest'
import { ErrorCode, McpError } from '@modelcontextprotocol/sdk/types.js'
import {
  ActionbookError,
  ErrorCodes,
  isActionbookError,
  toMcpError,
} from './errors.js'

describe('ActionbookError', () => {
  it('stores code and suggestion', () => {
    const error = new ActionbookError(
      ErrorCodes.INVALID_QUERY,
      'Query cannot be empty',
      'Provide a non-empty query'
    )

    expect(error.code).toBe(ErrorCodes.INVALID_QUERY)
    expect(error.suggestion).toBe('Provide a non-empty query')
    expect(isActionbookError(error)).toBe(true)
  })
})

describe('toMcpError', () => {
  it('passes through existing McpError', () => {
    const mcpError = new McpError(ErrorCode.InternalError, 'existing')
    expect(toMcpError(mcpError)).toBe(mcpError)
  })

  it('wraps ActionbookError', () => {
    const actionError = new ActionbookError(ErrorCodes.API_ERROR, 'API failed')
    const result = toMcpError(actionError)
    expect(result).toBeInstanceOf(McpError)
    expect(result.message).toContain(ErrorCodes.API_ERROR)
  })

  it('wraps generic error', () => {
    const result = toMcpError(new Error('boom'))
    expect(result).toBeInstanceOf(McpError)
    expect(result.code).toBe(ErrorCode.InternalError)
  })
})

import { McpError, ErrorCode } from '@modelcontextprotocol/sdk/types.js'
import { ActionbookError } from '@actionbookdev/sdk'

// Re-export from SDK
export {
  ActionbookError,
  ErrorCodes,
  isActionbookError,
} from '@actionbookdev/sdk'
export type { ActionbookErrorCode } from '@actionbookdev/sdk'

/**
 * Convert an error to MCP error format
 */
export function toMcpError(error: unknown): McpError {
  if (error instanceof McpError) {
    return error
  }

  if (error instanceof ActionbookError) {
    return new McpError(
      ErrorCode.InternalError,
      `${error.code}: ${error.message}`
    )
  }

  return new McpError(
    ErrorCode.InternalError,
    error instanceof Error ? error.message : 'Unknown error'
  )
}

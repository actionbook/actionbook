import {
  defineTool,
  getActionByIdSchema,
  getActionByIdDescription,
  type GetActionByIdInput,
  formatActionDetail,
} from '@actionbookdev/sdk'
import { ApiClient } from '../lib/api-client.js'

// Re-export for backwards compatibility
export { getActionByIdSchema as GetActionByIdInputSchema }
export type { GetActionByIdInput }

export function createGetActionByIdTool(
  apiClient: Pick<ApiClient, 'getActionById'>
) {
  return defineTool({
    name: 'get_action_by_id',
    description: getActionByIdDescription,
    inputSchema: getActionByIdSchema,
    handler: async (input: GetActionByIdInput): Promise<string> => {
      const detail = await apiClient.getActionById(input.id)
      return formatActionDetail(detail)
    },
  })
}

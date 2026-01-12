import {
  defineTool,
  searchActionsSchema,
  searchActionsDescription,
  type SearchActionsInput,
  formatSearchResults,
} from '@actionbookdev/sdk'
import { ApiClient } from '../lib/api-client.js'

// Re-export for backwards compatibility
export { searchActionsSchema as SearchActionsInputSchema }
export type { SearchActionsInput }

export function createSearchActionsTool(
  apiClient: Pick<ApiClient, 'searchActions'>
) {
  return defineTool({
    name: 'search_actions',
    description: searchActionsDescription,
    inputSchema: searchActionsSchema,
    handler: async (input: SearchActionsInput): Promise<string> => {
      const result = await apiClient.searchActions({
        query: input.query,
        type: input.type ?? 'hybrid',
        limit: input.limit ?? 5,
        sourceIds: input.sourceIds,
        minScore: input.minScore,
      })
      return formatSearchResults(result, input.query)
    },
  })
}

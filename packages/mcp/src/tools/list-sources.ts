import {
  defineTool,
  listSourcesSchema,
  listSourcesDescription,
  type ListSourcesInput,
  type SourceItem,
} from '@actionbookdev/sdk'
import { ApiClient } from '../lib/api-client.js'

// Re-export for backwards compatibility
export { listSourcesSchema as ListSourcesInputSchema }
export type { ListSourcesInput }

function formatSourceList(sources: SourceItem[]): string {
  if (sources.length === 0) {
    return 'No sources found.'
  }

  const lines: string[] = [`Found ${sources.length} source(s):`, '']

  for (const source of sources) {
    lines.push(`## Source ID: ${source.id}`)
    lines.push(`- **Name**: ${source.name}`)
    lines.push(`- **Base URL**: ${source.baseUrl}`)
    if (source.domain) {
      lines.push(`- **Domain**: ${source.domain}`)
    }
    if (source.description) {
      lines.push(`- **Description**: ${source.description}`)
    }
    if (source.tags && source.tags.length > 0) {
      lines.push(`- **Tags**: ${source.tags.join(', ')}`)
    }
    if (source.healthScore !== null) {
      lines.push(`- **Health Score**: ${source.healthScore}`)
    }
    lines.push('')
  }

  lines.push('---')
  lines.push(
    'Use `search_actions` with `sourceIds` parameter to filter actions by source.'
  )
  lines.push('Example: search_actions({ query: "login", sourceIds: "1,2" })')

  return lines.join('\n')
}

export function createListSourcesTool(
  apiClient: Pick<ApiClient, 'listSources'>
) {
  return defineTool({
    name: 'list_sources',
    description: listSourcesDescription,
    inputSchema: listSourcesSchema,
    handler: async (input: ListSourcesInput): Promise<string> => {
      const result = await apiClient.listSources(input.limit ?? 50)
      return formatSourceList(result.results)
    },
  })
}

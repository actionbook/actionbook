import { z } from 'zod'
import { defineTool } from './index.js'
import { ApiClient } from '../lib/api-client.js'
import { SourceItem } from '../lib/types.js'

export const ListSourcesInputSchema = z.object({
  limit: z
    .number()
    .int()
    .min(1)
    .max(200)
    .optional()
    .describe('Maximum number of sources to return (1-200, default: 50)'),
})

export type ListSourcesInput = z.infer<typeof ListSourcesInputSchema>

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
    description: `List all available sources (websites) in the Actionbook database.

Use this tool to:
- Discover what websites/sources are available
- Get source IDs for filtering search_actions
- View source metadata (name, URL, description, tags)

**Typical workflow:**
1. List sources: list_sources()
2. Note the source ID you want to search
3. Search actions: search_actions({ query: "login", sourceIds: "1" })

Returns source IDs, names, URLs, and metadata for each source.`,
    inputSchema: ListSourcesInputSchema,
    handler: async (input: ListSourcesInput): Promise<string> => {
      const result = await apiClient.listSources(input.limit ?? 50)
      return formatSourceList(result.results)
    },
  })
}

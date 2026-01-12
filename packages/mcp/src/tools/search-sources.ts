import {
  defineTool,
  searchSourcesSchema,
  searchSourcesDescription,
  type SearchSourcesInput,
  type SourceItem,
} from '@actionbookdev/sdk'
import { ApiClient } from '../lib/api-client.js'

// Re-export for backwards compatibility
export { searchSourcesSchema as SearchSourcesInputSchema }
export type { SearchSourcesInput }

function formatSourceSearchResults(sources: SourceItem[], query: string): string {
  if (sources.length === 0) {
    return `No sources found matching "${query}".`;
  }

  const lines: string[] = [
    `Found ${sources.length} source(s) matching "${query}":`,
    "",
  ];

  for (const source of sources) {
    lines.push(`## Source ID: ${source.id}`);
    lines.push(`- **Name**: ${source.name}`);
    lines.push(`- **Base URL**: ${source.baseUrl}`);
    if (source.domain) {
      lines.push(`- **Domain**: ${source.domain}`);
    }
    if (source.description) {
      lines.push(`- **Description**: ${source.description}`);
    }
    if (source.tags && source.tags.length > 0) {
      lines.push(`- **Tags**: ${source.tags.join(", ")}`);
    }
    if (source.healthScore !== null) {
      lines.push(`- **Health Score**: ${source.healthScore}`);
    }
    lines.push("");
  }

  lines.push("---");
  lines.push("Use `search_actions` with `sourceIds` parameter to filter actions by source.");
  lines.push("Example: search_actions({ query: \"button\", sourceIds: \"" + sources[0].id + "\" })");

  return lines.join("\n");
}

export function createSearchSourcesTool(
  apiClient: Pick<ApiClient, 'searchSources'>
) {
  return defineTool({
    name: 'search_sources',
    description: searchSourcesDescription,
    inputSchema: searchSourcesSchema,
    handler: async (input: SearchSourcesInput): Promise<string> => {
      const result = await apiClient.searchSources(input.query, input.limit ?? 10)
      return formatSourceSearchResults(result.results, input.query)
    },
  })
}

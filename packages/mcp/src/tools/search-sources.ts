import { z } from "zod";
import { defineTool } from "./index.js";
import { ApiClient } from "../lib/api-client.js";
import { SourceItem } from "../lib/types.js";

export const SearchSourcesInputSchema = z.object({
  query: z
    .string()
    .min(1, "Query cannot be empty")
    .max(200, "Query too long")
    .describe("Search keyword to find sources (searches name, description, domain, URL, and tags)"),
  limit: z
    .number()
    .int()
    .min(1)
    .max(100)
    .optional()
    .describe("Maximum number of results to return (1-100, default: 10)"),
});

export type SearchSourcesInput = z.infer<typeof SearchSourcesInputSchema>;

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

export function createSearchSourcesTool(apiClient: Pick<ApiClient, "searchSources">) {
  return defineTool({
    name: "search_sources",
    description: `Search for sources (websites) by keyword.

Use this tool to:
- Find specific websites/sources by name or domain
- Search by description or tags
- Get source IDs for filtering search_actions

**Search fields:**
- Source name
- Description
- Domain
- Base URL
- Tags

**Typical workflow:**
1. Search sources: search_sources({ query: "airbnb" })
2. Note the source ID from results
3. Search actions: search_actions({ query: "login", sourceIds: "1" })

**Example queries:**
- "airbnb" → find Airbnb source
- "linkedin" → find LinkedIn source
- "e-commerce" → find sources tagged with e-commerce

Returns matching source IDs, names, URLs, and metadata.`,
    inputSchema: SearchSourcesInputSchema,
    handler: async (input: SearchSourcesInput): Promise<string> => {
      const result = await apiClient.searchSources(input.query, input.limit ?? 10);
      return formatSourceSearchResults(result.results, input.query);
    },
  });
}

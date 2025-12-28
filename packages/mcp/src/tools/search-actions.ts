import { z } from "zod";
import { defineTool } from "./index.js";
import { ApiClient } from "../lib/api-client.js";
import { formatSearchResults } from "../lib/formatter.js";

export const SearchActionsInputSchema = z.object({
  query: z
    .string()
    .min(1, "Query cannot be empty")
    .max(200, "Query too long")
    .describe("Search keyword (e.g., 'company', 'login button', 'authentication')"),
  type: z
    .enum(["vector", "fulltext", "hybrid"])
    .optional()
    .describe("Search type: vector (semantic), fulltext (keyword), or hybrid (best quality, default: hybrid)"),
  limit: z
    .number()
    .int()
    .min(1)
    .max(100)
    .optional()
    .describe("Maximum number of results to return (1-100, default: 5)"),
  sourceIds: z
    .string()
    .optional()
    .describe("Comma-separated source IDs to filter by (e.g., '1,2,3')"),
  minScore: z
    .number()
    .min(0)
    .max(1)
    .optional()
    .describe("Minimum similarity score (0-1, e.g., 0.7 for high relevance only)"),
});

export type SearchActionsInput = z.infer<typeof SearchActionsInputSchema>;

export function createSearchActionsTool(apiClient: Pick<ApiClient, "searchActions">) {
  return defineTool({
    name: "search_actions",
    description: `Search for website actions by keyword using vector, fulltext, or hybrid search.

Use this tool to:
- Find page elements and their selectors
- Discover automation actions for specific websites
- Search by semantic meaning or exact keywords

**Search Types:**
1. **vector**: Semantic similarity search using embeddings
   - Best for: Natural language queries, finding similar concepts
   - Example: "How to authenticate users"

2. **fulltext**: PostgreSQL full-text keyword search
   - Best for: Exact keyword matching, faster searches
   - Example: "login button"

3. **hybrid** (default): Combines vector + fulltext with RRF fusion
   - Best for: Balanced quality and coverage
   - Recommended for most use cases

**Typical workflow:**
1. Search for actions: search_actions({ query: "company card" })
2. Get action_id from results (numeric ID)
3. Get full details: get_action_by_id({ id: 123 })
4. Use returned selectors with Playwright/browser automation

**Example queries:**
- "company card element" → find company card UI elements
- "expand button" → find expand/collapse buttons
- "authentication flow" → find login/auth related actions

**Tips:**
- Use sourceIds to filter by specific websites
- Use minScore (e.g., 0.7) to get only high-relevance results
- Increase limit if you need more results

Returns action_ids (numeric) with content previews and relevance scores.`,
    inputSchema: SearchActionsInputSchema,
    handler: async (input: SearchActionsInput): Promise<string> => {
      const result = await apiClient.searchActions({
        query: input.query,
        type: input.type ?? "hybrid",
        limit: input.limit ?? 5,
        sourceIds: input.sourceIds,
        minScore: input.minScore,
      });
      return formatSearchResults(result, input.query);
    },
  });
}

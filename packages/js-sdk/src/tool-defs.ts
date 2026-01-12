import { z } from 'zod'
import { zodToJsonSchema } from 'zod-to-json-schema'
import { toolInputToJsonSchema } from './schema-utils.js'

/**
 * Tool params interface with both JSON Schema and Zod formats
 */
export interface ToolParams<T extends z.ZodTypeAny> {
  /** JSON Schema format for OpenAI, Anthropic, Gemini SDKs */
  json: Record<string, unknown>
  /** Zod schema for Vercel AI SDK */
  zod: T
}

/**
 * Tool definition interface for MCP and other frameworks
 */
export interface ToolDefinition<T extends z.ZodTypeAny> {
  /** Tool name (snake_case, e.g., "search_actions") */
  name: string
  /** Tool description for LLM */
  description: string
  /** Input schema (Zod) */
  inputSchema: T
  /** Tool handler function */
  handler: (input: z.infer<T>) => Promise<string>
}

/**
 * Create a tool definition
 */
export function defineTool<T extends z.ZodTypeAny>(
  definition: ToolDefinition<T>
): ToolDefinition<T> {
  return definition
}

/**
 * Create tool params from a Zod schema
 */
function createParams<T extends z.ZodTypeAny>(schema: T): ToolParams<T> {
  return {
    json: zodToJsonSchema(schema, { $refStrategy: 'none' }),
    zod: schema,
  }
}

/**
 * Create tool params with cleaned JSON Schema (for Claude/MCP compatibility)
 */
export function createCleanParams<T extends z.ZodTypeAny>(
  schema: T
): ToolParams<T> {
  return {
    json: toolInputToJsonSchema(schema) as Record<string, unknown>,
    zod: schema,
  }
}

// ============================================
// searchActions tool definition
// ============================================

export const searchActionsSchema = z.object({
  query: z
    .string()
    .min(1, "Query cannot be empty")
    .max(200, "Query too long")
    .describe("Search keyword (e.g., 'airbnb search', 'login button', 'google login')"),
  type: z
    .enum(["vector", "fulltext", "hybrid"])
    .optional()
    .describe("Search type: vector (semantic), fulltext (keyword), or hybrid (default)"),
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

export type SearchActionsInput = z.infer<typeof searchActionsSchema>;

export const searchActionsDescription = `Search for action manuals by keyword.

Use this tool to find website actions, page elements, and their selectors for browser automation.

**Example queries:**
- "airbnb search" → find Airbnb search-related actions
- "google login" → find Google login actions
- "linkedin message" → find LinkedIn messaging actions

**Typical workflow:**
1. Search for actions: searchActions("airbnb search")
2. Get action_id from results
3. Get full details: getActionById(123)
4. Use returned selectors with Playwright/browser automation

Returns action IDs with content previews and relevance scores.`;

export const searchActionsParams = createParams(searchActionsSchema);

// ============================================
// getActionById tool definition
// ============================================

export const getActionByIdSchema = z.object({
  id: z
    .number()
    .int()
    .positive("Action ID must be a positive integer")
    .describe("Action ID (numeric, e.g., 123, 456)"),
});

export type GetActionByIdInput = z.infer<typeof getActionByIdSchema>;

export const getActionByIdDescription = `Get complete action details by action ID, including DOM selectors and step-by-step instructions.

**What you get:**
- Full action content/documentation
- Page element selectors (CSS, XPath)
- Element types and allowed methods (click, type, extract, etc.)
- Document metadata (title, URL)

**Use returned selectors with browser automation:**
\`\`\`javascript
const selector = '.search-button';
await page.locator(selector).click();
\`\`\`

**Typical workflow:**
1. Search for actions: searchActions("airbnb search")
2. Get action_id from results (e.g., 123)
3. Get full details: getActionById(123)
4. Extract selectors and use in automation`;

export const getActionByIdParams = createParams(getActionByIdSchema)

// ============================================
// listSources tool definition
// ============================================

export const listSourcesSchema = z.object({
  limit: z
    .number()
    .int()
    .min(1)
    .max(200)
    .optional()
    .describe('Maximum number of sources to return (1-200, default: 50)'),
})

export type ListSourcesInput = z.infer<typeof listSourcesSchema>

export const listSourcesDescription = `List all available sources (websites) in the Actionbook database.

Use this tool to:
- Discover what websites/sources are available
- Get source IDs for filtering search_actions
- View source metadata (name, URL, description, tags)

**Typical workflow:**
1. List sources: listSources()
2. Note the source ID you want to search
3. Search actions: searchActions({ query: "login", sourceIds: "1" })

Returns source IDs, names, URLs, and metadata for each source.`

export const listSourcesParams = createParams(listSourcesSchema)

// ============================================
// searchSources tool definition
// ============================================

export const searchSourcesSchema = z.object({
  query: z
    .string()
    .min(1, 'Query cannot be empty')
    .max(200, 'Query too long')
    .describe(
      'Search keyword to find sources (searches name, description, domain, URL, and tags)'
    ),
  limit: z
    .number()
    .int()
    .min(1)
    .max(100)
    .optional()
    .describe('Maximum number of results to return (1-100, default: 10)'),
})

export type SearchSourcesInput = z.infer<typeof searchSourcesSchema>

export const searchSourcesDescription = `Search for sources (websites) by keyword.

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
1. Search sources: searchSources({ query: "airbnb" })
2. Note the source ID from results
3. Search actions: searchActions({ query: "login", sourceIds: "1" })

**Example queries:**
- "airbnb" → find Airbnb source
- "linkedin" → find LinkedIn source
- "e-commerce" → find sources tagged with e-commerce

Returns matching source IDs, names, URLs, and metadata.`

export const searchSourcesParams = createParams(searchSourcesSchema)

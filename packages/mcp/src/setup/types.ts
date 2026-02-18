import { z } from 'zod'

/**
 * Supported target environments for agent setup.
 */
export const SetupTargetSchema = z.enum([
  'claude-code',
  'claude-desktop',
  'cursor',
  'vscode',
  'custom',
])

export type SetupTarget = z.infer<typeof SetupTargetSchema>

/**
 * Input schema for non-interactive agent setup.
 *
 * All fields are optional with sensible defaults so an agent can call
 * `npx @actionbookdev/mcp setup --json '{}'` for a zero-config setup.
 */
export const AgentSetupInputSchema = z.object({
  /** Target IDE / environment (default: auto-detect) */
  target: SetupTargetSchema.optional(),

  /** Actionbook API key (falls back to ACTIONBOOK_API_KEY env var) */
  apiKey: z.string().optional(),

  /** API base URL override */
  apiUrl: z.string().url().optional(),

  /** Project directory to configure (default: cwd) */
  projectDir: z.string().optional(),

  /** Bash permission patterns for .claude/settings.local.json */
  permissions: z.array(z.string()).optional(),

  /** Whether to write .env file with API key (default: false) */
  writeEnv: z.boolean().default(false),

  /** Whether to overwrite existing config files (default: false) */
  force: z.boolean().default(false),

  /** Transport type for MCP server */
  transport: z.enum(['stdio', 'http']).default('stdio'),

  /** MCP server command override (default: npx -y @actionbookdev/mcp@latest) */
  mcpCommand: z.string().optional(),

  /** MCP server args override */
  mcpArgs: z.array(z.string()).optional(),

  /** Output format */
  output: z.enum(['json', 'text']).default('json'),
})

export type AgentSetupInput = z.infer<typeof AgentSetupInputSchema>

/**
 * Result of a single file operation during setup.
 */
export interface FileResult {
  path: string
  action: 'created' | 'updated' | 'skipped'
  reason?: string
}

/**
 * Structured result returned by agent setup (machine-readable).
 */
export interface AgentSetupResult {
  success: boolean
  target: SetupTarget
  projectDir: string
  files: FileResult[]
  warnings: string[]
  /** Next steps for the agent / user */
  nextSteps: string[]
}

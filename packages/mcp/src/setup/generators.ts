import type { SetupTarget } from './types.js'

/**
 * MCP server entry for IDE configuration files.
 */
export interface McpServerEntry {
  command: string
  args: string[]
  env?: Record<string, string>
}

/**
 * Build the MCP server entry based on setup options.
 */
export function buildMcpServerEntry(options: {
  mcpCommand?: string
  mcpArgs?: string[]
  apiKey?: string
  apiUrl?: string
}): McpServerEntry {
  const command = options.mcpCommand ?? 'npx'
  const args = options.mcpArgs ?? ['-y', '@actionbookdev/mcp@latest']

  if (options.apiUrl) {
    args.push('--api-url', options.apiUrl)
  }

  const env: Record<string, string> = {}
  if (options.apiKey) {
    env.ACTIONBOOK_API_KEY = options.apiKey
  }

  return {
    command,
    args,
    ...(Object.keys(env).length > 0 ? { env } : {}),
  }
}

/**
 * Generate Claude Desktop config content (claude_desktop_config.json fragment).
 */
export function generateClaudeDesktopConfig(entry: McpServerEntry): object {
  return {
    mcpServers: {
      actionbook: entry,
    },
  }
}

/**
 * Generate Claude Code MCP config (mcp-servers.json or .mcp.json).
 */
export function generateClaudeCodeMcpConfig(entry: McpServerEntry): object {
  return {
    actionbook: entry,
  }
}

/**
 * Generate Claude Code permissions config (.claude/settings.local.json).
 */
export function generateClaudeCodePermissions(
  permissions: string[]
): object {
  return {
    permissions: {
      allow: permissions,
    },
  }
}

/**
 * Generate Cursor MCP config (.cursor/mcp.json).
 */
export function generateCursorMcpConfig(entry: McpServerEntry): object {
  return {
    mcpServers: {
      actionbook: entry,
    },
  }
}

/**
 * Generate VS Code MCP config (.vscode/mcp.json).
 */
export function generateVscodeMcpConfig(entry: McpServerEntry): object {
  return {
    servers: {
      actionbook: {
        type: 'stdio',
        command: entry.command,
        args: entry.args,
        ...(entry.env ? { env: entry.env } : {}),
      },
    },
  }
}

/**
 * Generate .env file content.
 */
export function generateEnvContent(options: {
  apiKey?: string
  apiUrl?: string
}): string {
  const lines: string[] = []
  if (options.apiKey) {
    lines.push(`ACTIONBOOK_API_KEY=${options.apiKey}`)
  }
  if (options.apiUrl) {
    lines.push(`ACTIONBOOK_API_URL=${options.apiUrl}`)
  }
  return lines.join('\n') + '\n'
}

/**
 * Get the MCP config file path for each target.
 */
export function getMcpConfigPath(target: SetupTarget): string {
  switch (target) {
    case 'claude-code':
      return '.mcp.json'
    case 'claude-desktop':
      return 'claude_desktop_config.json'
    case 'cursor':
      return '.cursor/mcp.json'
    case 'vscode':
      return '.vscode/mcp.json'
    case 'custom':
      return 'mcp-servers.json'
  }
}

/**
 * Get the permissions config file path (only relevant for Claude Code).
 */
export function getPermissionsConfigPath(
  target: SetupTarget
): string | null {
  if (target === 'claude-code') {
    return '.claude/settings.local.json'
  }
  return null
}

/**
 * Generate the MCP config object for the given target.
 */
export function generateMcpConfig(
  target: SetupTarget,
  entry: McpServerEntry
): object {
  switch (target) {
    case 'claude-code':
      return generateClaudeCodeMcpConfig(entry)
    case 'claude-desktop':
      return generateClaudeDesktopConfig(entry)
    case 'cursor':
      return generateCursorMcpConfig(entry)
    case 'vscode':
      return generateVscodeMcpConfig(entry)
    case 'custom':
      return generateClaudeCodeMcpConfig(entry)
  }
}

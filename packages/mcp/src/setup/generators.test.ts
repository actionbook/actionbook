import { describe, expect, it } from 'vitest'
import {
  buildMcpServerEntry,
  generateClaudeCodeMcpConfig,
  generateClaudeCodePermissions,
  generateClaudeDesktopConfig,
  generateCursorMcpConfig,
  generateVscodeMcpConfig,
  generateEnvContent,
  getMcpConfigPath,
  getPermissionsConfigPath,
} from './generators.js'

describe('buildMcpServerEntry', () => {
  it('returns default npx command', () => {
    const entry = buildMcpServerEntry({})
    expect(entry.command).toBe('npx')
    expect(entry.args).toEqual(['-y', '@actionbookdev/mcp@latest'])
    expect(entry.env).toBeUndefined()
  })

  it('includes env when apiKey is provided', () => {
    const entry = buildMcpServerEntry({ apiKey: 'ak-123' })
    expect(entry.env?.ACTIONBOOK_API_KEY).toBe('ak-123')
  })

  it('appends --api-url when apiUrl is provided', () => {
    const entry = buildMcpServerEntry({ apiUrl: 'https://custom.api.com' })
    expect(entry.args).toContain('--api-url')
    expect(entry.args).toContain('https://custom.api.com')
  })

  it('uses custom command and args', () => {
    const entry = buildMcpServerEntry({
      mcpCommand: 'node',
      mcpArgs: ['./dist/cli.js'],
    })
    expect(entry.command).toBe('node')
    expect(entry.args).toEqual(['./dist/cli.js'])
  })
})

describe('generateClaudeCodeMcpConfig', () => {
  it('wraps entry under actionbook key', () => {
    const entry = buildMcpServerEntry({})
    const config = generateClaudeCodeMcpConfig(entry)
    expect(config).toEqual({ actionbook: entry })
  })
})

describe('generateClaudeDesktopConfig', () => {
  it('wraps entry under mcpServers.actionbook', () => {
    const entry = buildMcpServerEntry({})
    const config = generateClaudeDesktopConfig(entry) as any
    expect(config.mcpServers.actionbook).toEqual(entry)
  })
})

describe('generateCursorMcpConfig', () => {
  it('wraps entry under mcpServers.actionbook', () => {
    const entry = buildMcpServerEntry({})
    const config = generateCursorMcpConfig(entry) as any
    expect(config.mcpServers.actionbook).toEqual(entry)
  })
})

describe('generateVscodeMcpConfig', () => {
  it('wraps entry under servers.actionbook with type', () => {
    const entry = buildMcpServerEntry({ apiKey: 'ak-test' })
    const config = generateVscodeMcpConfig(entry) as any
    expect(config.servers.actionbook.type).toBe('stdio')
    expect(config.servers.actionbook.command).toBe('npx')
    expect(config.servers.actionbook.env.ACTIONBOOK_API_KEY).toBe('ak-test')
  })
})

describe('generateClaudeCodePermissions', () => {
  it('creates permissions object', () => {
    const config = generateClaudeCodePermissions(['Bash(npx *)']) as any
    expect(config.permissions.allow).toEqual(['Bash(npx *)'])
  })
})

describe('generateEnvContent', () => {
  it('generates env lines', () => {
    const content = generateEnvContent({
      apiKey: 'ak-test',
      apiUrl: 'https://api.test.com',
    })
    expect(content).toContain('ACTIONBOOK_API_KEY=ak-test')
    expect(content).toContain('ACTIONBOOK_API_URL=https://api.test.com')
  })

  it('returns empty content when no values provided', () => {
    const content = generateEnvContent({})
    expect(content).toBe('\n')
  })
})

describe('getMcpConfigPath', () => {
  it('returns .mcp.json for claude-code', () => {
    expect(getMcpConfigPath('claude-code')).toBe('.mcp.json')
  })

  it('returns .cursor/mcp.json for cursor', () => {
    expect(getMcpConfigPath('cursor')).toBe('.cursor/mcp.json')
  })

  it('returns .vscode/mcp.json for vscode', () => {
    expect(getMcpConfigPath('vscode')).toBe('.vscode/mcp.json')
  })

  it('returns claude_desktop_config.json for claude-desktop', () => {
    expect(getMcpConfigPath('claude-desktop')).toBe('claude_desktop_config.json')
  })
})

describe('getPermissionsConfigPath', () => {
  it('returns path for claude-code', () => {
    expect(getPermissionsConfigPath('claude-code')).toBe(
      '.claude/settings.local.json'
    )
  })

  it('returns null for non-claude-code targets', () => {
    expect(getPermissionsConfigPath('cursor')).toBeNull()
    expect(getPermissionsConfigPath('vscode')).toBeNull()
    expect(getPermissionsConfigPath('claude-desktop')).toBeNull()
  })
})

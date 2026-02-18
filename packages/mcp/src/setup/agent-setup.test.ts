import { describe, expect, it, beforeEach, afterEach } from 'vitest'
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, existsSync, rmSync } from 'fs'
import path from 'path'
import os from 'os'
import { agentSetup, parseSetupArgs } from './agent-setup.js'

function createTmpDir(): string {
  return mkdtempSync(path.join(os.tmpdir(), 'actionbook-setup-test-'))
}

describe('parseSetupArgs', () => {
  it('parses --target flag', () => {
    const result = parseSetupArgs(['--target', 'claude-code'])
    expect(result.target).toBe('claude-code')
  })

  it('parses --api-key and --api-url', () => {
    const result = parseSetupArgs([
      '--api-key', 'ak-test',
      '--api-url', 'https://api.test.com',
    ])
    expect(result.apiKey).toBe('ak-test')
    expect(result.apiUrl).toBe('https://api.test.com')
  })

  it('parses --force and --write-env as booleans', () => {
    const result = parseSetupArgs(['--force', '--write-env'])
    expect(result.force).toBe(true)
    expect(result.writeEnv).toBe(true)
  })

  it('parses --json input', () => {
    const result = parseSetupArgs([
      '--json', '{"target":"cursor","apiKey":"ak-json"}',
    ])
    expect(result.target).toBe('cursor')
    expect(result.apiKey).toBe('ak-json')
  })

  it('parses --permissions as comma-separated list', () => {
    const result = parseSetupArgs([
      '--permissions', 'Bash(npx *),Bash(curl *)',
    ])
    expect(result.permissions).toEqual(['Bash(npx *)', 'Bash(curl *)'])
  })
})

describe('agentSetup', () => {
  let tmpDir: string

  beforeEach(() => {
    tmpDir = createTmpDir()
  })

  afterEach(() => {
    rmSync(tmpDir, { recursive: true, force: true })
  })

  it('creates MCP config for claude-code target', () => {
    const result = agentSetup({
      target: 'claude-code',
      projectDir: tmpDir,
      apiKey: 'ak-test',
    })

    expect(result.success).toBe(true)
    expect(result.target).toBe('claude-code')
    expect(result.files.length).toBeGreaterThanOrEqual(2)

    // Check .mcp.json was created
    const mcpConfig = JSON.parse(
      readFileSync(path.join(tmpDir, '.mcp.json'), 'utf-8')
    )
    expect(mcpConfig.actionbook).toBeDefined()
    expect(mcpConfig.actionbook.command).toBe('npx')
    expect(mcpConfig.actionbook.env?.ACTIONBOOK_API_KEY).toBe('ak-test')

    // Check .claude/settings.local.json was created
    const perms = JSON.parse(
      readFileSync(path.join(tmpDir, '.claude', 'settings.local.json'), 'utf-8')
    )
    expect(perms.permissions.allow).toBeDefined()
  })

  it('creates MCP config for cursor target', () => {
    const result = agentSetup({
      target: 'cursor',
      projectDir: tmpDir,
    })

    expect(result.success).toBe(true)
    expect(result.target).toBe('cursor')

    const mcpConfig = JSON.parse(
      readFileSync(path.join(tmpDir, '.cursor', 'mcp.json'), 'utf-8')
    )
    expect(mcpConfig.mcpServers.actionbook).toBeDefined()
  })

  it('creates MCP config for vscode target', () => {
    const result = agentSetup({
      target: 'vscode',
      projectDir: tmpDir,
    })

    expect(result.success).toBe(true)

    const mcpConfig = JSON.parse(
      readFileSync(path.join(tmpDir, '.vscode', 'mcp.json'), 'utf-8')
    )
    expect(mcpConfig.servers.actionbook).toBeDefined()
    expect(mcpConfig.servers.actionbook.type).toBe('stdio')
  })

  it('creates claude-desktop config', () => {
    const result = agentSetup({
      target: 'claude-desktop',
      projectDir: tmpDir,
    })

    expect(result.success).toBe(true)

    const config = JSON.parse(
      readFileSync(
        path.join(tmpDir, 'claude_desktop_config.json'),
        'utf-8'
      )
    )
    expect(config.mcpServers.actionbook).toBeDefined()
  })

  it('warns when no API key is provided', () => {
    const result = agentSetup({
      target: 'claude-code',
      projectDir: tmpDir,
    })

    expect(result.warnings).toContain(
      'No API key provided. Set ACTIONBOOK_API_KEY env var or pass --api-key.'
    )
  })

  it('writes .env file when requested', () => {
    const result = agentSetup({
      target: 'claude-code',
      projectDir: tmpDir,
      apiKey: 'ak-env-test',
      writeEnv: true,
    })

    expect(result.success).toBe(true)
    const envContent = readFileSync(path.join(tmpDir, '.env'), 'utf-8')
    expect(envContent).toContain('ACTIONBOOK_API_KEY=ak-env-test')
  })

  it('skips existing files without --force', () => {
    // Create existing .mcp.json
    writeFileSync(
      path.join(tmpDir, '.mcp.json'),
      JSON.stringify({ existing: true }, null, 2)
    )

    const result = agentSetup({
      target: 'claude-code',
      projectDir: tmpDir,
      apiKey: 'ak-test',
    })

    // Should merge, not skip (since existing file is valid JSON)
    const mcpConfig = JSON.parse(
      readFileSync(path.join(tmpDir, '.mcp.json'), 'utf-8')
    )
    expect(mcpConfig.existing).toBe(true)
    expect(mcpConfig.actionbook).toBeDefined()
  })

  it('overwrites existing files with --force', () => {
    writeFileSync(
      path.join(tmpDir, '.mcp.json'),
      JSON.stringify({ existing: true }, null, 2)
    )

    agentSetup({
      target: 'claude-code',
      projectDir: tmpDir,
      apiKey: 'ak-test',
      force: true,
    })

    const mcpConfig = JSON.parse(
      readFileSync(path.join(tmpDir, '.mcp.json'), 'utf-8')
    )
    expect(mcpConfig.existing).toBeUndefined()
    expect(mcpConfig.actionbook).toBeDefined()
  })

  it('auto-detects claude-code when .claude dir exists', () => {
    mkdirSync(path.join(tmpDir, '.claude'), { recursive: true })

    const result = agentSetup({ projectDir: tmpDir })
    expect(result.target).toBe('claude-code')
  })

  it('auto-detects cursor when .cursor dir exists', () => {
    mkdirSync(path.join(tmpDir, '.cursor'), { recursive: true })

    const result = agentSetup({ projectDir: tmpDir })
    expect(result.target).toBe('cursor')
  })

  it('auto-detects vscode when .vscode dir exists', () => {
    mkdirSync(path.join(tmpDir, '.vscode'), { recursive: true })

    const result = agentSetup({ projectDir: tmpDir })
    expect(result.target).toBe('vscode')
  })

  it('supports custom permissions', () => {
    agentSetup({
      target: 'claude-code',
      projectDir: tmpDir,
      permissions: ['Bash(agent-browser *)', 'Bash(curl *)'],
    })

    const perms = JSON.parse(
      readFileSync(path.join(tmpDir, '.claude', 'settings.local.json'), 'utf-8')
    )
    expect(perms.permissions.allow).toEqual([
      'Bash(agent-browser *)',
      'Bash(curl *)',
    ])
  })

  it('includes apiUrl in MCP args when provided', () => {
    agentSetup({
      target: 'claude-code',
      projectDir: tmpDir,
      apiUrl: 'https://custom.api.com',
    })

    const mcpConfig = JSON.parse(
      readFileSync(path.join(tmpDir, '.mcp.json'), 'utf-8')
    )
    expect(mcpConfig.actionbook.args).toContain('--api-url')
    expect(mcpConfig.actionbook.args).toContain('https://custom.api.com')
  })
})

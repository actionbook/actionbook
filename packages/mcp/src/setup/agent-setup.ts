import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'fs'
import path from 'path'
import {
  AgentSetupInputSchema,
  type AgentSetupInput,
  type AgentSetupResult,
  type FileResult,
  type SetupTarget,
} from './types.js'
import {
  buildMcpServerEntry,
  generateClaudeCodePermissions,
  generateEnvContent,
  generateMcpConfig,
  getMcpConfigPath,
  getPermissionsConfigPath,
} from './generators.js'

const DEFAULT_PERMISSIONS = ['Bash(npx @actionbookdev/mcp *)']

/**
 * Auto-detect the target environment by checking for IDE config directories.
 */
function detectTarget(projectDir: string): SetupTarget {
  if (existsSync(path.join(projectDir, '.claude'))) return 'claude-code'
  if (existsSync(path.join(projectDir, '.cursor'))) return 'cursor'
  if (existsSync(path.join(projectDir, '.vscode'))) return 'vscode'
  // Default to claude-code as the most common agent target
  return 'claude-code'
}

/**
 * Safely write a JSON config file. Merges with existing content if present.
 */
function writeJsonConfig(
  filePath: string,
  content: object,
  force: boolean
): FileResult {
  const dir = path.dirname(filePath)
  if (!existsSync(dir)) {
    mkdirSync(dir, { recursive: true })
  }

  if (existsSync(filePath) && !force) {
    try {
      const existing = JSON.parse(readFileSync(filePath, 'utf-8'))
      const merged = deepMerge(existing, content)
      writeFileSync(filePath, JSON.stringify(merged, null, 2) + '\n')
      return { path: filePath, action: 'updated' }
    } catch {
      // If we can't parse existing file, skip to avoid data loss
      return {
        path: filePath,
        action: 'skipped',
        reason: 'existing file could not be parsed; use --force to overwrite',
      }
    }
  }

  writeFileSync(filePath, JSON.stringify(content, null, 2) + '\n')
  return { path: filePath, action: existsSync(filePath) ? 'updated' : 'created' }
}

/**
 * Deep merge two objects. Arrays are replaced, not concatenated.
 */
function deepMerge(target: any, source: any): any {
  const result = { ...target }
  for (const key of Object.keys(source)) {
    if (
      source[key] &&
      typeof source[key] === 'object' &&
      !Array.isArray(source[key]) &&
      target[key] &&
      typeof target[key] === 'object' &&
      !Array.isArray(target[key])
    ) {
      result[key] = deepMerge(target[key], source[key])
    } else {
      result[key] = source[key]
    }
  }
  return result
}

/**
 * Run the non-interactive agent setup.
 *
 * This is the main entry point that agents call. It:
 * 1. Validates input via Zod schema
 * 2. Detects or uses the specified target environment
 * 3. Generates and writes all needed config files
 * 4. Returns a structured JSON result
 */
export function agentSetup(rawInput: unknown): AgentSetupResult {
  const input = AgentSetupInputSchema.parse(rawInput)
  const projectDir = path.resolve(input.projectDir ?? process.cwd())
  const target = input.target ?? detectTarget(projectDir)
  const apiKey = input.apiKey ?? process.env.ACTIONBOOK_API_KEY

  const files: FileResult[] = []
  const warnings: string[] = []
  const nextSteps: string[] = []

  // 1. Generate MCP server config
  const mcpEntry = buildMcpServerEntry({
    mcpCommand: input.mcpCommand,
    mcpArgs: input.mcpArgs,
    apiKey,
    apiUrl: input.apiUrl,
  })

  const mcpConfigRelPath = getMcpConfigPath(target)
  const mcpConfigAbsPath = path.join(projectDir, mcpConfigRelPath)
  const mcpContent = generateMcpConfig(target, mcpEntry)
  files.push(writeJsonConfig(mcpConfigAbsPath, mcpContent, input.force))

  // 2. Generate permissions config (Claude Code only)
  const permPath = getPermissionsConfigPath(target)
  if (permPath) {
    const permissions = input.permissions ?? DEFAULT_PERMISSIONS
    const permAbsPath = path.join(projectDir, permPath)
    const permContent = generateClaudeCodePermissions(permissions)
    files.push(writeJsonConfig(permAbsPath, permContent, input.force))
  }

  // 3. Write .env file if requested
  if (input.writeEnv && (apiKey || input.apiUrl)) {
    const envPath = path.join(projectDir, '.env')
    const envContent = generateEnvContent({
      apiKey,
      apiUrl: input.apiUrl,
    })

    if (existsSync(envPath) && !input.force) {
      files.push({
        path: envPath,
        action: 'skipped',
        reason: '.env already exists; use --force to overwrite',
      })
    } else {
      writeFileSync(envPath, envContent)
      files.push({
        path: envPath,
        action: existsSync(envPath) ? 'updated' : 'created',
      })
    }
  }

  // 4. Warnings
  if (!apiKey) {
    warnings.push(
      'No API key provided. Set ACTIONBOOK_API_KEY env var or pass --api-key.'
    )
  }

  // 5. Next steps
  if (target === 'claude-code') {
    nextSteps.push('Restart Claude Code to load the new MCP configuration.')
  } else if (target === 'claude-desktop') {
    nextSteps.push(
      'Copy the generated config into ~/Library/Application Support/Claude/claude_desktop_config.json (macOS) or %APPDATA%/Claude/claude_desktop_config.json (Windows).'
    )
  } else if (target === 'cursor') {
    nextSteps.push('Restart Cursor to load the new MCP configuration.')
  } else if (target === 'vscode') {
    nextSteps.push('Restart VS Code to load the new MCP configuration.')
  }

  nextSteps.push(
    'Verify setup by asking the agent to call search_actions with a test query.'
  )

  return {
    success: files.every((f) => f.action !== 'skipped'),
    target,
    projectDir,
    files,
    warnings,
    nextSteps,
  }
}

/**
 * Parse CLI arguments for the setup subcommand.
 */
export function parseSetupArgs(argv: string[]): Record<string, unknown> {
  const result: Record<string, unknown> = {}

  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i]
    const next = argv[i + 1]

    switch (arg) {
      case '--target':
        result.target = next
        i++
        break
      case '--api-key':
        result.apiKey = next
        i++
        break
      case '--api-url':
        result.apiUrl = next
        i++
        break
      case '--project-dir':
        result.projectDir = next
        i++
        break
      case '--permissions':
        result.permissions = next?.split(',').map((s) => s.trim())
        i++
        break
      case '--write-env':
        result.writeEnv = true
        break
      case '--force':
        result.force = true
        break
      case '--transport':
        result.transport = next
        i++
        break
      case '--mcp-command':
        result.mcpCommand = next
        i++
        break
      case '--mcp-args':
        result.mcpArgs = next?.split(',').map((s) => s.trim())
        i++
        break
      case '--output':
        result.output = next
        i++
        break
      case '--json': {
        // Accept full JSON input: --json '{"target":"claude-code",...}'
        try {
          const parsed = JSON.parse(next ?? '{}')
          Object.assign(result, parsed)
        } catch {
          // ignore parse errors, let Zod validation catch them
        }
        i++
        break
      }
      default:
        break
    }
  }

  return result
}

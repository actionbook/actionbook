#!/usr/bin/env node
import { ActionbookMcpServer } from './server.js'
import { loadConfig } from './lib/config.js'
import { createStdioTransport } from './transport/stdio.js'
import { createHttpTransport } from './transport/http.js'
import { logger } from './lib/logger.js'
import { agentSetup, parseSetupArgs } from './setup/index.js'

function isSetupCommand(argv: string[]): boolean {
  return argv.length > 0 && argv[0] === 'setup'
}

function runSetup(argv: string[]): void {
  const setupArgs = parseSetupArgs(argv)
  const outputFormat = (setupArgs.output as string) ?? 'json'

  try {
    const result = agentSetup(setupArgs)

    if (outputFormat === 'json') {
      process.stdout.write(JSON.stringify(result, null, 2) + '\n')
    } else {
      printSetupText(result)
    }

    process.exit(result.success ? 0 : 1)
  } catch (error) {
    if (outputFormat === 'json') {
      process.stdout.write(
        JSON.stringify({
          success: false,
          error: error instanceof Error ? error.message : String(error),
        }) + '\n'
      )
    } else {
      console.error(
        'Setup failed:',
        error instanceof Error ? error.message : error
      )
    }
    process.exit(1)
  }
}

function printSetupText(result: any): void {
  console.log(result.success ? 'Setup completed.' : 'Setup completed with warnings.')
  console.log(`Target: ${result.target}`)
  console.log(`Project: ${result.projectDir}`)
  console.log('')
  console.log('Files:')
  for (const f of result.files) {
    const suffix = f.reason ? ` (${f.reason})` : ''
    console.log(`  ${f.action}: ${f.path}${suffix}`)
  }
  if (result.warnings.length > 0) {
    console.log('')
    console.log('Warnings:')
    for (const w of result.warnings) {
      console.log(`  - ${w}`)
    }
  }
  if (result.nextSteps.length > 0) {
    console.log('')
    console.log('Next steps:')
    for (const s of result.nextSteps) {
      console.log(`  - ${s}`)
    }
  }
}

async function main(): Promise<void> {
  const argv = process.argv.slice(2)

  if (isSetupCommand(argv)) {
    runSetup(argv.slice(1))
    return
  }

  const config = loadConfig(argv)
  logger.info(`[Actionbook MCP] Starting with transport: ${config.transport}`)

  const server = new ActionbookMcpServer(config)
  const transport =
    config.transport === 'http'
      ? await createHttpTransport(config.http)
      : createStdioTransport()
  await server.start(transport)
}

main().catch((error) => {
  logger.error('[Actionbook MCP] Fatal error', error)
  process.exit(1)
})

#!/usr/bin/env node
import { ActionbookMcpServer } from './server.js'
import { loadConfig } from './lib/config.js'
import { createStdioTransport } from './transport/stdio.js'
import { createHttpTransport } from './transport/http.js'
import { logger } from './lib/logger.js'

async function main(): Promise<void> {
  const config = loadConfig(process.argv.slice(2))
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

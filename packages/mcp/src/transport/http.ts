import { SSEServerTransport } from '@modelcontextprotocol/sdk/server/sse.js'
import { logger } from '../lib/logger.js'

export interface HttpTransportConfig {
  port?: number
  host?: string
}

/**
 * HTTP/SSE transport (experimental). Primary mode is still stdio.
 */
export async function createHttpTransport(
  config: HttpTransportConfig = {}
): Promise<SSEServerTransport> {
  const port = config.port ?? 3001
  const host = config.host ?? '0.0.0.0'

  // SDK type declaration doesn't match actual constructor signature, using any wrapper (pending official type update)
  const SSETransport: any = SSEServerTransport as any
  const transport: SSEServerTransport = new SSETransport({
    port,
    hostname: host,
  })

  transport.onerror = (err) => logger.error('HTTP transport error', err)
  transport.onclose = () => logger.info('HTTP transport closed')

  await transport.start()
  logger.info(
    `[Actionbook MCP] HTTP transport listening on http://${host}:${port}`
  )
  return transport
}

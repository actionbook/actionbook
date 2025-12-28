import { beforeAll, afterAll, describe, expect, it } from 'vitest'
import http from 'http'
import { ActionbookMcpServer } from '../server.js'
import { ServerConfig } from '../lib/config.js'

const searchFixture = {
  success: true,
  query: 'example',
  results: [
    {
      action_id: 123,
      content: 'Example action content with elements',
      score: 0.95,
      createdAt: '2025-01-01T00:00:00Z',
    },
  ],
  count: 1,
  total: 1,
  hasMore: false,
}

const actionFixture = {
  action_id: 123,
  content: '# Example Action\n\nThis is example action content.',
  elements: JSON.stringify({
    example_button: {
      css_selector: '.example-button',
      element_type: 'button',
      allow_methods: ['click'],
    },
  }),
  createdAt: '2025-01-01T00:00:00Z',
  documentId: 1,
  documentTitle: 'Example Document',
  documentUrl: 'https://example.com',
  chunkIndex: 0,
  heading: 'Example Action',
  tokenCount: 100,
}

describe('ActionbookMcpServer integration with HTTP API', () => {
  let server: http.Server
  let baseUrl: string
  let serverRunning = false

  beforeAll(async () => {
    server = http.createServer((req, res) => {
      if (!req.url) {
        res.statusCode = 400
        return res.end()
      }

      const url = new URL(req.url, 'http://localhost')
      res.setHeader('content-type', 'application/json')

      if (url.pathname === '/api/health') {
        res.end(JSON.stringify({ status: 'ok' }))
        return
      }

      if (url.pathname === '/api/actions/search') {
        res.end(JSON.stringify(searchFixture))
        return
      }

      if (url.pathname.startsWith('/api/actions/')) {
        res.end(JSON.stringify(actionFixture))
        return
      }

      res.statusCode = 404
      res.end(JSON.stringify({ message: 'not found' }))
    })

    await new Promise<void>((resolve) => {
      server
        .listen(0, '127.0.0.1')
        .once('listening', () => {
          const address = server.address()
          if (address && typeof address === 'object') {
            baseUrl = `http://127.0.0.1:${address.port}`
            serverRunning = true
          }
          resolve()
        })
        .once('error', () => resolve())
    })
  })

  afterAll(async () => {
    if (!serverRunning) return
    await new Promise<void>((resolve, reject) => {
      server.close((err) => {
        if (err) reject(err)
        else resolve()
      })
    })
  })

  it('runs search_actions against HTTP API', async () => {
    if (!serverRunning) {
      return
    }
    const config: ServerConfig = {
      apiUrl: baseUrl,
      transport: 'stdio',
      logLevel: 'error',
      timeout: 2000,
      retry: { maxRetries: 0, retryDelay: 0 },
    }
    const mcpServer = new ActionbookMcpServer(config)

    const output = await mcpServer.callTool('search_actions', {
      query: 'example',
      type: 'hybrid',
      limit: 10,
    })

    expect(output).toContain('Action ID: 123')
    expect(output).toContain('Search Results')
  })

  it('runs get_action_by_id against HTTP API', async () => {
    if (!serverRunning) {
      return
    }
    const config: ServerConfig = {
      apiUrl: baseUrl,
      transport: 'stdio',
      logLevel: 'error',
      timeout: 2000,
      retry: { maxRetries: 0, retryDelay: 0 },
    }
    const mcpServer = new ActionbookMcpServer(config)

    const output = await mcpServer.callTool('get_action_by_id', {
      id: 123,
    })

    expect(output).toContain('Example Action')
    expect(output).toContain('UI Elements')
  })
})

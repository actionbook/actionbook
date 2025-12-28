import { describe, expect, it } from 'vitest'
import { loadConfig, ServerConfigSchema } from './config.js'

describe('ServerConfigSchema', () => {
  it('provides defaults', () => {
    const parsed = ServerConfigSchema.parse({})
    expect(parsed.apiUrl).toBe('https://api.actionbook.dev')
    expect(parsed.transport).toBe('stdio')
    expect(parsed.retry.maxRetries).toBe(3)
    expect(parsed.http?.port ?? 3001).toBe(3001)
  })
})

describe('loadConfig', () => {
  it('reads from environment', () => {
    const config = loadConfig([], {
      ACTIONBOOK_API_URL: 'https://api.example.com',
      ACTIONBOOK_API_KEY: 'secret',
      ACTIONBOOK_TRANSPORT: 'http',
      ACTIONBOOK_LOG_LEVEL: 'debug',
      ACTIONBOOK_HTTP_PORT: '3200',
    })

    expect(config.apiUrl).toBe('https://api.example.com')
    expect(config.apiKey).toBe('secret')
    expect(config.transport).toBe('http')
    expect(config.logLevel).toBe('debug')
    expect(config.http?.port).toBe(3200)
  })

  it('args override environment', () => {
    const config = loadConfig(
      ['--api-url', 'https://api.override.com', '--transport', 'stdio'],
      { ACTIONBOOK_API_URL: 'https://api.env.com' }
    )

    expect(config.apiUrl).toBe('https://api.override.com')
    expect(config.transport).toBe('stdio')
  })
})

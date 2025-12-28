import { describe, expect, it, beforeAll } from 'vitest'
import { ApiClient } from '../lib/api-client.js'
import { ActionbookError } from '../lib/errors.js'

const API_URL =
  process.env.ACTIONBOOK_API_URL ||
  process.env.ACTIONBOOK_REAL_API_URL ||
  'http://localhost:3100'

let apiAvailable = false

async function tryHealth(client: ApiClient): Promise<boolean> {
  try {
    return await client.healthCheck()
  } catch {
    return false
  }
}

describe('Real API integration (skippable)', () => {
  beforeAll(async () => {
    const client = new ApiClient(API_URL, {
      retry: { maxRetries: 0 },
      timeoutMs: 3000,
    })
    apiAvailable = await tryHealth(client)
    if (!apiAvailable) {
      // Health check failed, still try real requests for debugging
      // eslint-disable-next-line no-console
      console.warn('API health check failed; attempting real calls anyway')
      apiAvailable = true
    }
  })

  it('search_actions hits real API', async () => {
    const client = new ApiClient(API_URL, {
      retry: { maxRetries: 0 },
      timeoutMs: 5000,
    })
    try {
      const result = await client.searchActions({
        query: 'airbnb',
        limit: 3,
        page: 1,
      })
      expect(result.results.length).toBeGreaterThan(0)
    } catch (error) {
      if (error instanceof Error && error.message.includes('fetch failed')) {
        return // treat as skip when fetch not reachable
      }
      if (
        error instanceof ActionbookError &&
        error.code === 'API_ERROR' &&
        error.message.includes('401')
      ) {
        return // skip when API requires authentication
      }
      throw error
    }
  })

  it('get_action_by_id hits real API', async () => {
    const client = new ApiClient(API_URL, {
      retry: { maxRetries: 0 },
      timeoutMs: 5000,
    })
    // Use first search result ID; adjust based on actual data
    try {
      const search = await client.searchActions({
        query: 'airbnb',
        limit: 1,
        page: 1,
      })
      const first = search.results[0]
      expect(first).toBeDefined()
      const content = await client.getActionById(first.action_id)
      expect(content.action_id).toBe(first.action_id)
    } catch (error) {
      if (error instanceof Error && error.message.includes('fetch failed')) {
        return // treat as skip when fetch not reachable
      }
      if (
        error instanceof ActionbookError &&
        error.code === 'API_ERROR' &&
        error.message.includes('401')
      ) {
        return // skip when API requires authentication
      }
      throw error
    }
  })
})

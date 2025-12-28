import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiClient } from './api-client.js'
import { ActionbookError, ErrorCodes } from './errors.js'

const API_URL = 'http://localhost:3100'

// Create mock function using vi.hoisted to avoid hoisting issues
const fetchMock = vi.hoisted(() => vi.fn())

// Mock undici module
vi.mock('undici', () => ({
  fetch: fetchMock,
  ProxyAgent: vi.fn(),
}))

describe('ApiClient', () => {
  beforeEach(() => {
    fetchMock.mockReset()
  })

  it('calls search endpoint with query params', async () => {
    const client = new ApiClient(API_URL, { retry: { maxRetries: 0 } })
    fetchMock.mockResolvedValue(
      new Response(
        JSON.stringify({
          success: true,
          query: 'company',
          results: [],
          count: 0,
          total: 0,
          hasMore: false,
        }),
        { status: 200 }
      )
    )

    await client.searchActions({ query: 'company', type: 'hybrid', limit: 10 })
    const url = new URL(fetchMock.mock.calls[0][0] as string)
    expect(url.pathname).toBe('/api/actions/search')
    expect(url.searchParams.get('q')).toBe('company')
    expect(url.searchParams.get('type')).toBe('hybrid')
    expect(url.searchParams.get('limit')).toBe('10')
  })

  it('gets action by numeric id', async () => {
    const client = new ApiClient(API_URL, { retry: { maxRetries: 0 } })
    fetchMock.mockResolvedValue(
      new Response(
        JSON.stringify({
          action_id: 123,
          content: 'Test content',
          elements: null,
          createdAt: '2025-12-05T00:00:00.000Z',
          documentId: 1,
          documentTitle: 'Test Doc',
          documentUrl: 'https://example.com',
          chunkIndex: 0,
          heading: 'Test',
          tokenCount: 100,
        }),
        {
          status: 200,
        }
      )
    )

    const result = await client.getActionById(123)
    expect(result.action_id).toBe(123)
    expect(result.content).toBe('Test content')
  })

  it('throws ActionbookError on http error', async () => {
    const client = new ApiClient(API_URL, {
      retry: { maxRetries: 0 },
      timeoutMs: 100,
    })
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify({ message: 'not found' }), { status: 404 })
    )

    await expect(client.getActionById(999999)).rejects.toBeInstanceOf(
      ActionbookError
    )
    try {
      await client.getActionById(999999)
    } catch (error) {
      if (error instanceof ActionbookError) {
        expect(error.code).toBe(ErrorCodes.NOT_FOUND)
      }
    }
  })
})

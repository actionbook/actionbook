import { describe, expect, it } from 'vitest'
import { ChunkSearchResult, ChunkActionDetail } from './types.js'
import {
  formatSearchResults,
  formatActionDetail,
  formatErrorMessage,
} from './formatter.js'
import { ActionbookError, ErrorCodes } from './errors.js'

describe('formatSearchResults', () => {
  it('formats list with pagination hint', () => {
    const result: ChunkSearchResult = {
      success: true,
      query: 'company',
      results: [
        {
          action_id: 1,
          content: 'First result',
          score: 0.95,
          createdAt: '2025-12-05T00:00:00.000Z',
        },
        {
          action_id: 2,
          content: 'Second result',
          score: 0.85,
          createdAt: '2025-12-05T00:00:00.000Z',
        },
      ],
      count: 2,
      total: 2,
      hasMore: true,
    }

    const markdown = formatSearchResults(result, 'company')

    expect(markdown).toContain('Search Results for "company"')
    expect(markdown).toContain('## 1. Action ID: 1')
    expect(markdown).toContain('More results available.')
  })

  it('shows suggestion when empty', () => {
    const result: ChunkSearchResult = {
      success: true,
      query: 'missing',
      results: [],
      count: 0,
      total: 0,
      hasMore: false,
    }

    const markdown = formatSearchResults(result, 'missing')
    expect(markdown).toContain('No actions found')
    expect(markdown).toContain('Try broader search terms')
  })
})

describe('formatActionDetail', () => {
  it('formats action detail with elements', () => {
    const detail: ChunkActionDetail = {
      action_id: 123,
      content: 'Test content',
      elements: JSON.stringify({
        test_button: {
          css_selector: '.test-button',
          element_type: 'button',
          allow_methods: ['click'],
        },
      }),
      createdAt: '2025-12-05T00:00:00.000Z',
      documentId: 1,
      documentTitle: 'Test Doc',
      documentUrl: 'https://example.com',
      chunkIndex: 0,
      heading: 'Test',
      tokenCount: 100,
    }

    const markdown = formatActionDetail(detail)
    expect(markdown).toContain('UI Elements')
    expect(markdown).toContain('```json')
    expect(markdown).toContain('test_button')
    expect(markdown).toContain('.test-button')
  })

  it('handles action without elements', () => {
    const detail: ChunkActionDetail = {
      action_id: 123,
      content: 'Test content without elements',
      elements: null,
      createdAt: '2025-12-05T00:00:00.000Z',
      documentId: 1,
      documentTitle: 'Test Doc',
      documentUrl: 'https://example.com',
      chunkIndex: 0,
      heading: 'Test',
      tokenCount: 100,
    }

    const markdown = formatActionDetail(detail)
    expect(markdown).not.toContain('UI Elements')
    expect(markdown).toContain('Test content without elements')
  })
})

describe('formatErrorMessage', () => {
  it('formats ActionbookError', () => {
    const error = new ActionbookError(
      ErrorCodes.INVALID_QUERY,
      'Query invalid',
      'Provide query'
    )
    const markdown = formatErrorMessage(error)
    expect(markdown).toContain('INVALID_QUERY')
    expect(markdown).toContain('Provide query')
  })
})

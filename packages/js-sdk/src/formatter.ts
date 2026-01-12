import { ActionbookError } from './errors.js'
import { ChunkSearchResult, ChunkActionDetail, ParsedElements } from './types.js'

/**
 * Format search results as Markdown
 */
export function formatSearchResults(
  result: ChunkSearchResult,
  query: string
): string {
  const { results, count, hasMore } = result

  if (!results.length) {
    return `# Search Results for "${query}"

No actions found matching your query.

**Suggestions:**
- Try broader search terms
- Check spelling
- Use different search type (vector/fulltext/hybrid)`
  }

  const lines: string[] = [
    `# Search Results for "${query}"`,
    '',
    `Found ${count} result${count > 1 ? 's' : ''}:`,
    '',
  ]

  results.forEach((action, index) => {
    const num = index + 1
    lines.push(...formatChunkMeta(action, num), '')
  })

  if (hasMore) {
    lines.push(
      '---',
      '**More results available.** Refine your search or increase limit.',
      ''
    )
  }

  lines.push(
    '**Next step**: Use `get_action_by_id` with an action_id above to get full details.'
  )
  return lines.join('\n')
}

function formatChunkMeta(
  action: ChunkSearchResult['results'][0],
  position: number
): string[] {
  const lines: string[] = [
    `## ${position}. Action ID: ${action.action_id}`,
    `- **Score**: ${(action.score ?? 0).toFixed(3)}`,
    `- **Created**: ${formatDate(action.createdAt)}`,
    '',
    '**Preview:**',
    truncateContent(action.content, 200),
  ]

  return lines
}

/**
 * Format action detail as Markdown
 */
export function formatActionDetail(detail: ChunkActionDetail): string {
  const lines: string[] = [
    `# ${detail.heading || detail.documentTitle}`,
    '',
    '## Metadata',
    `- **Action ID**: ${detail.action_id}`,
    `- **Document**: ${detail.documentTitle}`,
    `- **URL**: ${detail.documentUrl}`,
    `- **Chunk Index**: ${detail.chunkIndex}`,
    `- **Token Count**: ${detail.tokenCount}`,
    `- **Created**: ${formatDate(detail.createdAt)}`,
    '',
  ]

  // Add content section
  lines.push('## Content', '', detail.content, '')

  // Add UI elements section if available
  if (detail.elements) {
    try {
      const elements: ParsedElements = JSON.parse(detail.elements)
      lines.push('## UI Elements', '')
      lines.push('```json')
      lines.push(JSON.stringify(elements, null, 2))
      lines.push('```')
      lines.push('')
    } catch (error) {
      lines.push('## UI Elements', '', '_(Failed to parse elements data)_', '')
    }
  }

  return lines.join('\n')
}

/**
 * Truncate content to a maximum length
 */
export function truncateContent(content: string, maxLength: number): string {
  if (content.length <= maxLength) {
    return content
  }
  return content.substring(0, maxLength) + '...'
}

/**
 * Format date string for display
 */
export function formatDate(dateStr: string): string {
  const date = new Date(dateStr)
  return Number.isNaN(date.valueOf())
    ? dateStr
    : date.toLocaleDateString('en-US', {
        year: 'numeric',
        month: 'short',
        day: 'numeric',
        hour: '2-digit',
        minute: '2-digit',
      })
}

/**
 * Format error message for display
 */
export function formatErrorMessage(error: unknown): string {
  if (error instanceof ActionbookError) {
    const lines = [`## Error: ${error.code}`, '', error.message]
    if (error.suggestion) {
      lines.push('', '**Suggestion:**', error.suggestion)
    }
    return lines.join('\n')
  }

  if (error instanceof Error) {
    return `## Internal Error\n\n${error.message}`
  }

  return '## Internal Error\n\nUnknown error'
}

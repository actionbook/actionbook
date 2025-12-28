import {
  getDb,
  chunks,
  documents,
  sources,
  sql,
  eq,
  and,
  inArray,
} from '@actionbookdev/db'
import type {
  HeadingItem,
  BreadcrumbItem as DbBreadcrumbItem,
} from '@actionbookdev/db'
import { getEmbedding } from './embedding'
import type { SearchResult, SearchType, BreadcrumbItem } from '../types/search'

interface SearchOptions {
  sourceIds?: number[]
  limit: number
}

// Helper to convert HeadingItem[] to string[]
function headingHierarchyToStrings(hierarchy: HeadingItem[] | null): string[] {
  if (!hierarchy) return []
  return hierarchy.map((h) => h.text)
}

// Helper to convert BreadcrumbItem types
function toBreadcrumbItems(items: DbBreadcrumbItem[] | null): BreadcrumbItem[] {
  if (!items) return []
  return items.map((item) => ({ title: item.title, url: item.url }))
}

/**
 * Vector similarity search using pgvector
 * Only searches chunks belonging to the active version of each source
 */
export async function vectorSearch(
  embedding: number[],
  options: SearchOptions
): Promise<SearchResult[]> {
  const { sourceIds, limit } = options
  const embeddingStr = `[${embedding.join(',')}]`
  const db = getDb()

  // Build conditions
  // Only search chunks that belong to the active version (source.currentVersionId)
  const conditions = [
    eq(documents.status, 'active'),
    sql`${chunks.embedding} IS NOT NULL`,
    // Filter by active version: chunk's sourceVersionId must match source's currentVersionId
    sql`${chunks.sourceVersionId} = ${sources.currentVersionId}`,
  ]

  if (sourceIds && sourceIds.length > 0) {
    conditions.push(inArray(documents.sourceId, sourceIds))
  }

  const results = await db
    .select({
      chunkId: chunks.id,
      content: chunks.content,
      headingHierarchy: chunks.headingHierarchy,
      createdAt: chunks.createdAt,
      documentId: documents.id,
      title: documents.title,
      url: documents.url,
      breadcrumb: documents.breadcrumb,
      score: sql<number>`1 - (${chunks.embedding} <=> ${embeddingStr}::vector)`,
    })
    .from(chunks)
    .innerJoin(documents, eq(chunks.documentId, documents.id))
    .innerJoin(sources, eq(documents.sourceId, sources.id))
    .where(and(...conditions))
    .orderBy(sql`${chunks.embedding} <=> ${embeddingStr}::vector`)
    .limit(limit)

  return results.map((row) => ({
    chunkId: row.chunkId,
    documentId: row.documentId,
    content: row.content,
    title: row.title || '',
    url: row.url,
    headingHierarchy: headingHierarchyToStrings(row.headingHierarchy),
    breadcrumb: toBreadcrumbItems(row.breadcrumb),
    score: row.score,
    createdAt: row.createdAt,
  }))
}

/**
 * Full-text search using PostgreSQL tsvector on chunks.content
 * Only searches chunks belonging to the active version of each source
 *
 * Note: PostgreSQL full-text search functions (ts_rank_cd, plainto_tsquery, @@, to_tsvector)
 * are not natively supported by Drizzle ORM, so we use sql template literals
 * for those parts while using ORM for structure (join, where).
 */
export async function fulltextSearch(
  query: string,
  options: SearchOptions
): Promise<SearchResult[]> {
  const { sourceIds, limit } = options
  const db = getDb()

  // Create the tsquery once for reuse
  const tsQuery = sql`plainto_tsquery('english', ${query})`

  // Build conditions
  // Only search chunks that belong to the active version (source.currentVersionId)
  const conditions = [
    sql`to_tsvector('english', ${chunks.content}) @@ ${tsQuery}`,
    eq(documents.status, 'active'),
    // Filter by active version: chunk's sourceVersionId must match source's currentVersionId
    sql`${chunks.sourceVersionId} = ${sources.currentVersionId}`,
  ]

  if (sourceIds && sourceIds.length > 0) {
    conditions.push(inArray(documents.sourceId, sourceIds))
  }

  // Search directly on chunks.content using to_tsvector
  // Generated SQL:
  //   SELECT chunks.id as chunk_id, chunks.content, chunks.heading_hierarchy, chunks.created_at,
  //          documents.id as document_id, documents.title, documents.url, documents.breadcrumb,
  //          ts_rank_cd(to_tsvector('english', chunks.content), plainto_tsquery('english', $1)) as rank
  //   FROM chunks
  //   INNER JOIN documents ON chunks.document_id = documents.id
  //   INNER JOIN sources ON documents.source_id = sources.id
  //   WHERE to_tsvector('english', chunks.content) @@ plainto_tsquery('english', $1)
  //     AND documents.status = 'active'
  //     AND chunks.source_version_id = sources.current_version_id
  //     [AND documents.source_id IN ($2, $3, ...)]
  //   ORDER BY ts_rank_cd(to_tsvector('english', chunks.content), plainto_tsquery('english', $1)) DESC
  //   LIMIT $N
  const results = await db
    .select({
      chunkId: chunks.id,
      content: chunks.content,
      headingHierarchy: chunks.headingHierarchy,
      createdAt: chunks.createdAt,
      documentId: documents.id,
      title: documents.title,
      url: documents.url,
      breadcrumb: documents.breadcrumb,
      score: sql<number>`ts_rank_cd(to_tsvector('english', ${chunks.content}), ${tsQuery})`,
    })
    .from(chunks)
    .innerJoin(documents, eq(chunks.documentId, documents.id))
    .innerJoin(sources, eq(documents.sourceId, sources.id))
    .where(and(...conditions))
    .orderBy(
      sql`ts_rank_cd(to_tsvector('english', ${chunks.content}), ${tsQuery}) DESC`
    )
    .limit(limit)

  return results.map((row) => ({
    chunkId: row.chunkId,
    documentId: row.documentId,
    content: row.content,
    title: row.title || '',
    url: row.url,
    headingHierarchy: headingHierarchyToStrings(row.headingHierarchy),
    breadcrumb: toBreadcrumbItems(row.breadcrumb),
    score: row.score,
    createdAt: row.createdAt,
  }))
}

/**
 * Hybrid search using Reciprocal Rank Fusion (RRF)
 */
export async function hybridSearch(
  query: string,
  options: SearchOptions
): Promise<SearchResult[]> {
  const expandedLimit = options.limit * 2
  const embedding = await getEmbedding(query)

  const [vectorResults, ftResults] = await Promise.all([
    vectorSearch(embedding, { ...options, limit: expandedLimit }),
    fulltextSearch(query, { ...options, limit: expandedLimit }),
  ])

  // RRF fusion with k=60
  const k = 60
  const scores = new Map<string, { score: number; result: SearchResult }>()

  vectorResults.forEach((result, rank) => {
    const key = `${result.documentId}-${result.chunkId}`
    const rrfScore = 1 / (k + rank + 1)
    scores.set(key, { score: rrfScore, result })
  })

  ftResults.forEach((result, rank) => {
    const key = `${result.documentId}-${result.chunkId}`
    const rrfScore = 1 / (k + rank + 1)
    const existing = scores.get(key)

    if (existing) {
      existing.score += rrfScore
    } else {
      scores.set(key, { score: rrfScore, result })
    }
  })

  return Array.from(scores.values())
    .sort((a, b) => b.score - a.score)
    .slice(0, options.limit)
    .map(({ score, result }) => ({ ...result, score }))
}

/**
 * Main search function
 */
export async function search(
  query: string,
  options: {
    searchType?: SearchType
    limit?: number
    sourceIds?: number[]
    minScore?: number
  }
): Promise<SearchResult[]> {
  const { searchType = 'hybrid', limit = 10, sourceIds, minScore = 0 } = options

  let results: SearchResult[]

  switch (searchType) {
    case 'vector': {
      const embedding = await getEmbedding(query)
      results = await vectorSearch(embedding, { sourceIds, limit })
      break
    }
    case 'fulltext':
      results = await fulltextSearch(query, { sourceIds, limit })
      break
    case 'hybrid':
    default:
      results = await hybridSearch(query, { sourceIds, limit })
      break
  }

  if (minScore > 0) {
    results = results.filter((r) => r.score >= minScore)
  }

  return results
}

/**
 * Get formatted context for LLM consumption
 */
export async function getContextForLLM(
  query: string,
  options: {
    searchType?: SearchType
    limit?: number
    sourceIds?: number[]
    maxTokens?: number
  }
): Promise<string> {
  const { maxTokens = 4000, ...searchOptions } = options
  const results = await search(query, { ...searchOptions, limit: 20 })

  const contextParts: string[] = []
  let totalTokens = 0

  for (const result of results) {
    // Rough token estimate: ~4 chars per token
    const chunkTokens = Math.ceil(result.content.length / 4)
    if (totalTokens + chunkTokens > maxTokens) break

    const hierarchy = result.headingHierarchy.join(' > ')
    const context = `
---
Source: ${result.title}
Section: ${hierarchy}
URL: ${result.url}

${result.content}
---`

    contextParts.push(context)
    totalTokens += chunkTokens
  }

  return contextParts.join('\n')
}

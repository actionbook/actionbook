import { NextRequest, NextResponse } from 'next/server'
import { getDb, sources } from '@actionbookdev/db'
import { ilike, or, sql } from 'drizzle-orm'

/**
 * Source search result
 */
interface SourceSearchResult {
  id: number
  name: string
  baseUrl: string
  description: string | null
  domain: string | null
  tags: string[]
  healthScore: number | null
  lastCrawledAt: string | null
  createdAt: string
}

interface SourceSearchResponse {
  success: boolean
  query: string
  results?: SourceSearchResult[]
  count?: number
  error?: string
}

/**
 * GET /api/sources/search?q=query&limit=10
 *
 * Search sources by name, description, domain, or tags
 */
export async function GET(
  request: NextRequest
): Promise<NextResponse<SourceSearchResponse>> {
  try {
    const { searchParams } = new URL(request.url)
    const query = searchParams.get('q')

    if (!query) {
      return NextResponse.json(
        {
          success: false,
          query: '',
          results: [],
          count: 0,
          error: 'q parameter is required',
        },
        { status: 400 }
      )
    }

    // Parse and validate parameters
    const limit = Math.min(
      Math.max(parseInt(searchParams.get('limit') || '10', 10), 1),
      100
    )

    const db = getDb()
    const searchPattern = `%${query}%`

    // Search by name, description, domain, or tags (using ILIKE for case-insensitive)
    const results = await db
      .select({
        id: sources.id,
        name: sources.name,
        baseUrl: sources.baseUrl,
        description: sources.description,
        domain: sources.domain,
        tags: sources.tags,
        healthScore: sources.healthScore,
        lastCrawledAt: sources.lastCrawledAt,
        createdAt: sources.createdAt,
      })
      .from(sources)
      .where(
        or(
          ilike(sources.name, searchPattern),
          ilike(sources.description, searchPattern),
          ilike(sources.domain, searchPattern),
          ilike(sources.baseUrl, searchPattern),
          // Search in tags array (jsonb)
          sql`${sources.tags}::text ILIKE ${searchPattern}`
        )
      )
      .limit(limit)

    // Map to response format
    const mappedResults: SourceSearchResult[] = results.map((row) => ({
      id: row.id,
      name: row.name,
      baseUrl: row.baseUrl,
      description: row.description,
      domain: row.domain,
      tags: row.tags || [],
      healthScore: row.healthScore,
      lastCrawledAt: row.lastCrawledAt?.toISOString() || null,
      createdAt: row.createdAt.toISOString(),
    }))

    return NextResponse.json({
      success: true,
      query,
      results: mappedResults,
      count: mappedResults.length,
    })
  } catch (error) {
    console.error('Source search API error:', error)
    return NextResponse.json(
      {
        success: false,
        query: '',
        results: [],
        count: 0,
        error: error instanceof Error ? error.message : 'Internal server error',
      },
      { status: 500 }
    )
  }
}

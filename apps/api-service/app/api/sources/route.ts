import { NextRequest, NextResponse } from 'next/server'
import { getDb, sources } from '@actionbookdev/db'
import { desc } from 'drizzle-orm'

/**
 * Source item in list response
 */
interface SourceItem {
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

interface SourceListResponse {
  success: boolean
  results?: SourceItem[]
  count?: number
  error?: string
}

/**
 * GET /api/sources?limit=50
 *
 * List all sources, ordered by creation date (newest first)
 */
export async function GET(
  request: NextRequest
): Promise<NextResponse<SourceListResponse>> {
  try {
    const { searchParams } = new URL(request.url)

    // Parse and validate parameters
    const limit = Math.min(
      Math.max(parseInt(searchParams.get('limit') || '50', 10), 1),
      200
    )

    const db = getDb()

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
      .orderBy(desc(sources.createdAt))
      .limit(limit)

    // Map to response format
    const mappedResults: SourceItem[] = results.map((row) => ({
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
      results: mappedResults,
      count: mappedResults.length,
    })
  } catch (error) {
    console.error('Source list API error:', error)
    return NextResponse.json(
      {
        success: false,
        results: [],
        count: 0,
        error: error instanceof Error ? error.message : 'Internal server error',
      },
      { status: 500 }
    )
  }
}

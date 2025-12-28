/**
 * Get Action by ID Endpoint
 * GET /api/actions/:id
 *
 * Now supports chunk_id as action_id
 * Returns complete chunk information from database
 */

import { NextRequest, NextResponse } from 'next/server'
import type { ApiError } from '@/lib/types'
import { getDb, chunks, documents, eq } from '@actionbookdev/db'

interface ActionContent {
  action_id: number
  content: string
  elements: string | null
  createdAt: string
  documentId: number
  documentTitle: string
  documentUrl: string
  chunkIndex: number
  heading?: string | null
  tokenCount: number
}

export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ id: string[] }> }
): Promise<NextResponse<ActionContent | ApiError>> {
  const { id } = (await params) as { id: string[] }
  const actionId = id.join('/')

  // Parse action_id as integer (chunk_id)
  const chunkId = parseInt(actionId, 10)

  if (isNaN(chunkId)) {
    return NextResponse.json(
      {
        error: 'INVALID_ID',
        code: '400',
        message: `Invalid action ID '${actionId}'. Expected a numeric chunk ID.`,
        suggestion: 'Use search to find valid action IDs.',
      },
      { status: 400 }
    )
  }

  try {
    const db = getDb()

    // Query chunk with document info, elements now comes from chunks table
    const results = await db
      .select({
        chunkId: chunks.id,
        content: chunks.content,
        elements: chunks.elements,
        createdAt: chunks.createdAt,
        documentId: documents.id,
        documentTitle: documents.title,
        documentUrl: documents.url,
        chunkIndex: chunks.chunkIndex,
        heading: chunks.heading,
        tokenCount: chunks.tokenCount,
      })
      .from(chunks)
      .innerJoin(documents, eq(chunks.documentId, documents.id))
      .where(eq(chunks.id, chunkId))
      .limit(1)

    if (results.length === 0) {
      return NextResponse.json(
        {
          error: 'NOT_FOUND',
          code: '404',
          message: `Action with ID '${chunkId}' not found`,
          suggestion: 'Use search to find available actions.',
        },
        { status: 404 }
      )
    }

    const chunk = results[0]

    return NextResponse.json({
      action_id: chunk.chunkId,
      content: chunk.content,
      elements: chunk.elements,
      createdAt: chunk.createdAt.toISOString(),
      documentId: chunk.documentId,
      documentTitle: chunk.documentTitle || '',
      documentUrl: chunk.documentUrl,
      chunkIndex: chunk.chunkIndex,
      heading: chunk.heading,
      tokenCount: chunk.tokenCount,
    })
  } catch (error) {
    console.error('Get action by ID error:', error)
    return NextResponse.json(
      {
        error: 'INTERNAL_ERROR',
        code: '500',
        message:
          error instanceof Error ? error.message : 'Internal server error',
      },
      { status: 500 }
    )
  }
}

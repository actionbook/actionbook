import { NextRequest, NextResponse } from 'next/server'
import {
  getDb,
  sourceVersions,
  documents,
  chunks,
  eq,
  and,
  inArray,
} from '@actionbookdev/db'
import crypto from 'crypto'

interface ChunkInput {
  documentId: number
  content: string
  contentHash?: string
  embedding?: number[]
  chunkIndex?: number
  startChar?: number
  endChar?: number
  heading?: string
  headingHierarchy?: Array<{ level: number; text: string }>
  tokenCount?: number
  embeddingModel?: string
  elements?: string
  metadata?: Record<string, unknown>
}

interface UploadChunksRequest {
  chunks: ChunkInput[]
}

interface UploadChunksResponse {
  success: boolean
  data?: {
    processedDocIds: number[]
    insertedCount: number
  }
  error?: {
    code: string
    message: string
  }
}

/**
 * Generate content hash for deduplication
 */
function generateContentHash(content: string): string {
  return crypto
    .createHash('sha256')
    .update(content)
    .digest('hex')
    .substring(0, 64)
}

/**
 * POST /api/sync/versions/:versionId/chunks
 *
 * Batch upload chunks to a version
 * - Idempotent: For each documentId in the request, delete existing chunks then insert new ones
 * - This allows retry without duplicate data
 */
export async function POST(
  request: NextRequest,
  { params }: { params: Promise<{ versionId: string }> }
): Promise<NextResponse<UploadChunksResponse>> {
  try {
    const { versionId: versionIdStr } = await params
    const versionId = parseInt(versionIdStr, 10)

    if (isNaN(versionId)) {
      return NextResponse.json(
        {
          success: false,
          error: {
            code: 'INVALID_VERSION_ID',
            message: 'Version ID must be a number',
          },
        },
        { status: 400 }
      )
    }

    const body = (await request.json()) as UploadChunksRequest
    const { chunks: inputChunks } = body

    if (
      !inputChunks ||
      !Array.isArray(inputChunks) ||
      inputChunks.length === 0
    ) {
      return NextResponse.json(
        {
          success: false,
          error: {
            code: 'INVALID_REQUEST',
            message: 'Chunks array is required and must not be empty',
          },
        },
        { status: 400 }
      )
    }

    const db = getDb()

    // 1. Verify version exists and is in 'building' status
    const version = await db
      .select()
      .from(sourceVersions)
      .where(eq(sourceVersions.id, versionId))
      .limit(1)
      .then((rows) => rows[0])

    if (!version) {
      return NextResponse.json(
        {
          success: false,
          error: {
            code: 'VERSION_NOT_FOUND',
            message: `Version ${versionId} not found`,
          },
        },
        { status: 404 }
      )
    }

    if (version.status !== 'building') {
      return NextResponse.json(
        {
          success: false,
          error: {
            code: 'VERSION_LOCKED',
            message: `Version ${versionId} is ${version.status}, cannot upload chunks`,
          },
        },
        { status: 409 }
      )
    }

    // 2. Group chunks by documentId
    const chunksByDocId = new Map<number, ChunkInput[]>()
    for (const chunk of inputChunks) {
      const existing = chunksByDocId.get(chunk.documentId) || []
      existing.push(chunk)
      chunksByDocId.set(chunk.documentId, existing)
    }

    const docIds = Array.from(chunksByDocId.keys())

    // 3. Verify all documents exist and belong to this version
    const validDocs = await db
      .select({ id: documents.id })
      .from(documents)
      .where(
        and(
          eq(documents.sourceVersionId, versionId),
          inArray(documents.id, docIds)
        )
      )

    const validDocIds = new Set(validDocs.map((d) => d.id))
    const invalidDocIds = docIds.filter((id) => !validDocIds.has(id))

    if (invalidDocIds.length > 0) {
      return NextResponse.json(
        {
          success: false,
          error: {
            code: 'INVALID_DOCUMENT_IDS',
            message: `Documents not found or not in this version: ${invalidDocIds.join(
              ', '
            )}`,
          },
        },
        { status: 400 }
      )
    }

    // 4. For each document: delete existing chunks, then insert new ones
    const processedDocIds: number[] = []
    let insertedCount = 0

    for (const docId of docIds) {
      const docChunks = chunksByDocId.get(docId)!

      // Delete existing chunks for this document
      await db.delete(chunks).where(eq(chunks.documentId, docId))

      // Insert new chunks
      const chunkValues = docChunks.map((chunk, idx) => ({
        documentId: docId,
        content: chunk.content,
        contentHash: chunk.contentHash || generateContentHash(chunk.content),
        chunkIndex: chunk.chunkIndex ?? idx,
        startChar: chunk.startChar ?? 0,
        endChar: chunk.endChar ?? chunk.content.length,
        heading: chunk.heading || null,
        headingHierarchy: chunk.headingHierarchy || [],
        tokenCount: chunk.tokenCount ?? Math.ceil(chunk.content.length / 4),
        embedding: chunk.embedding || null,
        embeddingModel: chunk.embeddingModel || null,
        elements: chunk.elements || null,
      }))

      if (chunkValues.length > 0) {
        await db.insert(chunks).values(chunkValues)
        insertedCount += chunkValues.length
      }

      processedDocIds.push(docId)
    }

    return NextResponse.json({
      success: true,
      data: {
        processedDocIds,
        insertedCount,
      },
    })
  } catch (error) {
    console.error('Upload chunks error:', error)
    return NextResponse.json(
      {
        success: false,
        error: {
          code: 'INTERNAL_ERROR',
          message:
            error instanceof Error ? error.message : 'Internal server error',
        },
      },
      { status: 500 }
    )
  }
}

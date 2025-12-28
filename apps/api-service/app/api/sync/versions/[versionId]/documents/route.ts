import { NextRequest, NextResponse } from 'next/server'
import { getDb, sourceVersions, documents, eq, and } from '@actionbookdev/db'
import crypto from 'crypto'

interface BreadcrumbItem {
  title: string
  url: string
}

interface DocumentInput {
  localId: number
  url: string
  urlHash?: string
  title?: string
  description?: string
  contentText?: string
  contentHtml?: string
  contentMd?: string
  elements?: string
  breadcrumb?: BreadcrumbItem[]
  wordCount?: number
  language?: string
  contentHash?: string
  depth?: number
  parentId?: number
  metadata?: Record<string, unknown>
}

interface UploadDocumentsRequest {
  documents: DocumentInput[]
}

interface UploadDocumentsResponse {
  success: boolean
  data?: {
    mapping: Array<{ localId: number; prodId: number }>
  }
  error?: {
    code: string
    message: string
  }
}

/**
 * Generate URL hash if not provided
 */
function generateUrlHash(url: string): string {
  return crypto.createHash('sha256').update(url).digest('hex').substring(0, 64)
}

/**
 * POST /api/sync/versions/:versionId/documents
 *
 * Batch upload documents to a version
 * - Supports upsert by urlHash (idempotent)
 * - Returns mapping of localId -> prodId
 */
export async function POST(
  request: NextRequest,
  { params }: { params: Promise<{ versionId: string }> }
): Promise<NextResponse<UploadDocumentsResponse>> {
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

    const body = (await request.json()) as UploadDocumentsRequest
    const { documents: inputDocs } = body

    if (!inputDocs || !Array.isArray(inputDocs) || inputDocs.length === 0) {
      return NextResponse.json(
        {
          success: false,
          error: {
            code: 'INVALID_REQUEST',
            message: 'Documents array is required and must not be empty',
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
            message: `Version ${versionId} is ${version.status}, cannot upload documents`,
          },
        },
        { status: 409 }
      )
    }

    // 2. Process documents with upsert logic
    const mapping: Array<{ localId: number; prodId: number }> = []

    for (const doc of inputDocs) {
      const urlHash = doc.urlHash || generateUrlHash(doc.url)

      // Check if document already exists in this version
      const existingDoc = await db
        .select({ id: documents.id })
        .from(documents)
        .where(
          and(
            eq(documents.sourceVersionId, versionId),
            eq(documents.urlHash, urlHash)
          )
        )
        .limit(1)
        .then((rows) => rows[0])

      let prodId: number

      if (existingDoc) {
        // Update existing document
        await db
          .update(documents)
          .set({
            url: doc.url,
            title: doc.title || null,
            description: doc.description || null,
            contentText: doc.contentText || null,
            contentHtml: doc.contentHtml || null,
            contentMd: doc.contentMd || null,
            elements: doc.elements || null,
            breadcrumb: doc.breadcrumb || [],
            wordCount: doc.wordCount || null,
            language: doc.language || 'en',
            contentHash: doc.contentHash || null,
            depth: doc.depth ?? 0,
            parentId: doc.parentId || null,
            updatedAt: new Date(),
          })
          .where(eq(documents.id, existingDoc.id))

        prodId = existingDoc.id
      } else {
        // Insert new document
        const [newDoc] = await db
          .insert(documents)
          .values({
            sourceId: version.sourceId,
            sourceVersionId: versionId,
            url: doc.url,
            urlHash,
            title: doc.title || null,
            description: doc.description || null,
            contentText: doc.contentText || null,
            contentHtml: doc.contentHtml || null,
            contentMd: doc.contentMd || null,
            elements: doc.elements || null,
            breadcrumb: doc.breadcrumb || [],
            wordCount: doc.wordCount || null,
            language: doc.language || 'en',
            contentHash: doc.contentHash || null,
            depth: doc.depth ?? 0,
            parentId: doc.parentId || null,
          })
          .returning({ id: documents.id })

        prodId = newDoc.id
      }

      mapping.push({ localId: doc.localId, prodId })
    }

    return NextResponse.json({
      success: true,
      data: { mapping },
    })
  } catch (error) {
    console.error('Upload documents error:', error)
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

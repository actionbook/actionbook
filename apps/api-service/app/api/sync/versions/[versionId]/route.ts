import { NextRequest, NextResponse } from 'next/server'
import { getDb, sourceVersions, sources, eq } from '@actionbookdev/db'

interface DeleteVersionResponse {
  success: boolean
  message?: string
  error?: {
    code: string
    message: string
  }
}

interface GetVersionResponse {
  success: boolean
  data?: {
    id: number
    sourceId: number
    versionNumber: number
    status: string
    commitMessage: string | null
    createdBy: string | null
    createdAt: string
    publishedAt: string | null
  }
  error?: string
}

/**
 * GET /api/sync/versions/:versionId
 *
 * Get version details
 */
export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ versionId: string }> }
): Promise<NextResponse<GetVersionResponse>> {
  try {
    const { versionId: versionIdStr } = await params
    const versionId = parseInt(versionIdStr, 10)

    if (isNaN(versionId)) {
      return NextResponse.json(
        {
          success: false,
          error: 'Version ID must be a number',
        },
        { status: 400 }
      )
    }

    const db = getDb()

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
          error: `Version ${versionId} not found`,
        },
        { status: 404 }
      )
    }

    return NextResponse.json({
      success: true,
      data: {
        id: version.id,
        sourceId: version.sourceId,
        versionNumber: version.versionNumber,
        status: version.status,
        commitMessage: version.commitMessage,
        createdBy: version.createdBy,
        createdAt: version.createdAt.toISOString(),
        publishedAt: version.publishedAt?.toISOString() || null,
      },
    })
  } catch (error) {
    console.error('Get version error:', error)
    return NextResponse.json(
      {
        success: false,
        error: error instanceof Error ? error.message : 'Internal server error',
      },
      { status: 500 }
    )
  }
}

/**
 * DELETE /api/sync/versions/:versionId
 *
 * Delete a version and its associated data
 * - Cannot delete version referenced by source.currentVersionId (regardless of status)
 * - Cascade deletes documents and chunks
 */
export async function DELETE(
  request: NextRequest,
  { params }: { params: Promise<{ versionId: string }> }
): Promise<NextResponse<DeleteVersionResponse>> {
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

    const db = getDb()

    // 1. Get the version
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

    // 2. Check if this version is referenced by source.currentVersionId
    // This check is based on the actual pointer, not version.status, to handle
    // edge cases where status might be stale (e.g., crash during publish)
    const source = await db
      .select({ currentVersionId: sources.currentVersionId })
      .from(sources)
      .where(eq(sources.id, version.sourceId))
      .limit(1)
      .then((rows) => rows[0])

    if (source?.currentVersionId === versionId) {
      return NextResponse.json(
        {
          success: false,
          error: {
            code: 'CANNOT_DELETE_REFERENCED',
            message:
              'Cannot delete version that is currently referenced by source. Publish a new version first.',
          },
        },
        { status: 409 }
      )
    }

    // 3. Delete the version (cascade will delete documents and chunks)
    await db.delete(sourceVersions).where(eq(sourceVersions.id, versionId))

    return NextResponse.json({
      success: true,
      message: `Version ${versionId} and associated data deleted`,
    })
  } catch (error) {
    console.error('Delete version error:', error)
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

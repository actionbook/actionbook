import { NextRequest, NextResponse } from 'next/server'
import { getDb, sourceVersions, sources, eq } from '@actionbookdev/db'

interface PublishVersionResponse {
  success: boolean
  data?: {
    activeVersionId: number
    archivedVersionId: number | null
    publishedAt: string
  }
  error?: {
    code: string
    message: string
  }
}

/**
 * POST /api/sync/versions/:versionId/publish
 *
 * Publish a version (atomic switch)
 * - Set new version to 'active'
 * - Set old active version to 'archived'
 * - Update source's currentVersionId
 */
export async function POST(
  request: NextRequest,
  { params }: { params: Promise<{ versionId: string }> }
): Promise<NextResponse<PublishVersionResponse>> {
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

    // 1. Get the version to publish
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
            message: `Version ${versionId} is ${version.status}, can only publish 'building' versions`,
          },
        },
        { status: 409 }
      )
    }

    // 2. Get source and current active version
    const source = await db
      .select()
      .from(sources)
      .where(eq(sources.id, version.sourceId))
      .limit(1)
      .then((rows) => rows[0])

    if (!source) {
      return NextResponse.json(
        {
          success: false,
          error: {
            code: 'SOURCE_NOT_FOUND',
            message: `Source ${version.sourceId} not found`,
          },
        },
        { status: 404 }
      )
    }

    const oldActiveVersionId = source.currentVersionId
    const publishedAt = new Date()

    // 3. Perform atomic switch in transaction
    await db.transaction(async (tx) => {
      // 3a. Archive old active version (if exists)
      if (oldActiveVersionId) {
        await tx
          .update(sourceVersions)
          .set({ status: 'archived' })
          .where(eq(sourceVersions.id, oldActiveVersionId))
      }

      // 3b. Set new version to active
      await tx
        .update(sourceVersions)
        .set({
          status: 'active',
          publishedAt,
        })
        .where(eq(sourceVersions.id, versionId))

      // 3c. Update source's currentVersionId
      await tx
        .update(sources)
        .set({
          currentVersionId: versionId,
          updatedAt: publishedAt,
        })
        .where(eq(sources.id, version.sourceId))
    })

    return NextResponse.json({
      success: true,
      data: {
        activeVersionId: versionId,
        archivedVersionId: oldActiveVersionId,
        publishedAt: publishedAt.toISOString(),
      },
    })
  } catch (error) {
    console.error('Publish version error:', error)
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

import { NextRequest, NextResponse } from 'next/server'
import {
  getDb,
  sources,
  sourceVersions,
  eq,
  and,
  desc,
} from '@actionbookdev/db'

interface CreateVersionRequest {
  commitMessage?: string
  createdBy?: string
}

interface CreateVersionResponse {
  success: boolean
  data?: {
    sourceId: number
    versionId: number
    versionNumber: number
    status: string
  }
  error?: {
    code: string
    message: string
    data?: Record<string, unknown>
  }
}

interface ListVersionsResponse {
  success: boolean
  data?: {
    sourceId: number
    sourceName: string
    currentVersionId: number | null
    versions: Array<{
      id: number
      versionNumber: number
      status: string
      commitMessage: string | null
      createdBy: string | null
      createdAt: string
      publishedAt: string | null
    }>
  }
  error?: string
}

/**
 * POST /api/sync/sources/:sourceName/versions
 *
 * Create a new version for syncing (Init phase)
 * - If source doesn't exist, create it
 * - Check for concurrent sync (only one building version allowed)
 * - Create new version with status 'building'
 */
export async function POST(
  request: NextRequest,
  { params }: { params: Promise<{ sourceName: string }> }
): Promise<NextResponse<CreateVersionResponse>> {
  try {
    const { sourceName } = await params
    const body = (await request.json()) as CreateVersionRequest
    const { commitMessage, createdBy } = body

    const db = getDb()

    // 1. Find or create source
    let source = await db
      .select()
      .from(sources)
      .where(eq(sources.name, sourceName))
      .limit(1)
      .then((rows) => rows[0])

    if (!source) {
      // Auto-create source
      const [newSource] = await db
        .insert(sources)
        .values({
          name: sourceName,
          baseUrl: `https://${sourceName}`,
        })
        .returning()
      source = newSource
    }

    // 2. Check for existing building version (concurrent sync protection)
    const existingBuildingVersion = await db
      .select()
      .from(sourceVersions)
      .where(
        and(
          eq(sourceVersions.sourceId, source.id),
          eq(sourceVersions.status, 'building')
        )
      )
      .limit(1)
      .then((rows) => rows[0])

    if (existingBuildingVersion) {
      return NextResponse.json(
        {
          success: false,
          error: {
            code: 'SYNC_IN_PROGRESS',
            message: 'Another sync is already in progress for this source',
            data: {
              existingVersionId: existingBuildingVersion.id,
              existingVersionNumber: existingBuildingVersion.versionNumber,
              createdAt: existingBuildingVersion.createdAt.toISOString(),
            },
          },
        },
        { status: 409 }
      )
    }

    // 3. Get next version number
    const latestVersion = await db
      .select({ versionNumber: sourceVersions.versionNumber })
      .from(sourceVersions)
      .where(eq(sourceVersions.sourceId, source.id))
      .orderBy(desc(sourceVersions.versionNumber))
      .limit(1)
      .then((rows) => rows[0])

    const nextVersionNumber = (latestVersion?.versionNumber ?? 0) + 1

    // 4. Create new version
    const [newVersion] = await db
      .insert(sourceVersions)
      .values({
        sourceId: source.id,
        versionNumber: nextVersionNumber,
        status: 'building',
        commitMessage: commitMessage || null,
        createdBy: createdBy || null,
      })
      .returning()

    return NextResponse.json(
      {
        success: true,
        data: {
          sourceId: source.id,
          versionId: newVersion.id,
          versionNumber: newVersion.versionNumber,
          status: newVersion.status,
        },
      },
      { status: 201 }
    )
  } catch (error) {
    console.error('Create version error:', error)
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

/**
 * GET /api/sync/sources/:sourceName/versions
 *
 * List all versions for a source
 */
export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ sourceName: string }> }
): Promise<NextResponse<ListVersionsResponse>> {
  try {
    const { sourceName } = await params
    const db = getDb()

    // Find source
    const source = await db
      .select()
      .from(sources)
      .where(eq(sources.name, sourceName))
      .limit(1)
      .then((rows) => rows[0])

    if (!source) {
      return NextResponse.json(
        {
          success: false,
          error: `Source '${sourceName}' not found`,
        },
        { status: 404 }
      )
    }

    // Get all versions
    const versions = await db
      .select()
      .from(sourceVersions)
      .where(eq(sourceVersions.sourceId, source.id))
      .orderBy(desc(sourceVersions.versionNumber))

    return NextResponse.json({
      success: true,
      data: {
        sourceId: source.id,
        sourceName: source.name,
        currentVersionId: source.currentVersionId,
        versions: versions.map((v) => ({
          id: v.id,
          versionNumber: v.versionNumber,
          status: v.status,
          commitMessage: v.commitMessage,
          createdBy: v.createdBy,
          createdAt: v.createdAt.toISOString(),
          publishedAt: v.publishedAt?.toISOString() || null,
        })),
      },
    })
  } catch (error) {
    console.error('List versions error:', error)
    return NextResponse.json(
      {
        success: false,
        error: error instanceof Error ? error.message : 'Internal server error',
      },
      { status: 500 }
    )
  }
}

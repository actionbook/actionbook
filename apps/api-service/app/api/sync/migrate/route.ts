import { NextRequest, NextResponse } from 'next/server'
import {
  getDb,
  sources,
  sourceVersions,
  documents,
  eq,
  isNull,
  sql,
} from '@actionbookdev/db'

interface MigrateResponse {
  success: boolean
  data?: {
    migratedSources: number
    skippedSources: number
    updatedDocuments: number
    details: Array<{
      sourceId: number
      sourceName: string
      versionId: number
      documentsUpdated: number
    }>
  }
  error?: {
    code: string
    message: string
  }
}

/**
 * POST /api/sync/migrate
 *
 * Migrate existing data to support versioning (Blue/Green deployment)
 * For each source without a current_version_id:
 * 1. Create an initial version (status = 'active')
 * 2. Update all documents (source_version_id IS NULL) to point to this version
 * 3. Update source.current_version_id to point to this version
 *
 * This is idempotent - running it multiple times is safe.
 */
export async function POST(
  _request: NextRequest
): Promise<NextResponse<MigrateResponse>> {
  try {
    const db = getDb()

    // Find sources without current_version_id
    const sourcesToMigrate = await db
      .select()
      .from(sources)
      .where(isNull(sources.currentVersionId))

    if (sourcesToMigrate.length === 0) {
      return NextResponse.json({
        success: true,
        data: {
          migratedSources: 0,
          skippedSources: 0,
          updatedDocuments: 0,
          details: [],
        },
      })
    }

    const details: Array<{
      sourceId: number
      sourceName: string
      versionId: number
      documentsUpdated: number
    }> = []
    let totalUpdatedDocuments = 0

    for (const source of sourcesToMigrate) {
      // Check if there's already a version for this source
      const existingVersion = await db
        .select()
        .from(sourceVersions)
        .where(eq(sourceVersions.sourceId, source.id))
        .limit(1)
        .then((rows) => rows[0])

      let versionId: number

      if (existingVersion) {
        // Use existing version
        versionId = existingVersion.id
      } else {
        // Create initial version
        const [newVersion] = await db
          .insert(sourceVersions)
          .values({
            sourceId: source.id,
            versionNumber: 1,
            status: 'active',
            commitMessage: 'Initial migration from legacy data',
            createdBy: 'system/migrate',
            publishedAt: new Date(),
          })
          .returning()
        versionId = newVersion.id
      }

      // Update documents without source_version_id
      const updateResult = await db
        .update(documents)
        .set({ sourceVersionId: versionId })
        .where(
          sql`${documents.sourceId} = ${source.id} AND ${documents.sourceVersionId} IS NULL`
        )

      const documentsUpdated = updateResult.rowCount ?? 0
      totalUpdatedDocuments += documentsUpdated

      // Update source.current_version_id
      await db
        .update(sources)
        .set({ currentVersionId: versionId })
        .where(eq(sources.id, source.id))

      details.push({
        sourceId: source.id,
        sourceName: source.name,
        versionId,
        documentsUpdated,
      })
    }

    return NextResponse.json({
      success: true,
      data: {
        migratedSources: details.length,
        skippedSources: 0,
        updatedDocuments: totalUpdatedDocuments,
        details,
      },
    })
  } catch (error) {
    console.error('Migration error:', error)
    return NextResponse.json(
      {
        success: false,
        error: {
          code: 'MIGRATION_ERROR',
          message: error instanceof Error ? error.message : 'Migration failed',
        },
      },
      { status: 500 }
    )
  }
}

/**
 * GET /api/sync/migrate
 *
 * Check migration status - show sources that need migration
 */
export async function GET(_request: NextRequest): Promise<NextResponse> {
  try {
    const db = getDb()

    // Find sources without current_version_id
    const sourcesWithoutVersion = await db
      .select({
        id: sources.id,
        name: sources.name,
        currentVersionId: sources.currentVersionId,
      })
      .from(sources)
      .where(isNull(sources.currentVersionId))

    // Count documents without source_version_id
    const orphanDocuments = await db
      .select({ count: sql<number>`count(*)` })
      .from(documents)
      .where(isNull(documents.sourceVersionId))
      .then((rows) => Number(rows[0]?.count ?? 0))

    return NextResponse.json({
      success: true,
      data: {
        needsMigration: sourcesWithoutVersion.length > 0 || orphanDocuments > 0,
        sourcesWithoutVersion: sourcesWithoutVersion.length,
        orphanDocuments,
        sources: sourcesWithoutVersion,
      },
    })
  } catch (error) {
    console.error('Check migration status error:', error)
    return NextResponse.json(
      {
        success: false,
        error:
          error instanceof Error
            ? error.message
            : 'Failed to check migration status',
      },
      { status: 500 }
    )
  }
}

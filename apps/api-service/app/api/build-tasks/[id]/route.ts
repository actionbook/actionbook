import { NextRequest } from 'next/server'
import { getDb, buildTasks } from '@actionbookdev/db'
import type { BuildTaskConfig } from '@actionbookdev/db'
import { eq } from 'drizzle-orm'
import {
  successResponse,
  invalidRequestResponse,
  notFoundResponse,
  internalErrorResponse,
} from '@/lib/response'

// ============================================================================
// Types
// ============================================================================

interface BuildTaskDetail {
  id: number
  sourceId: number | null
  sourceUrl: string
  sourceName: string | null
  sourceCategory: string
  stage: string
  stageStatus: string
  config: BuildTaskConfig | null
  createdAt: string
  updatedAt: string
  knowledgeStartedAt: string | null
  knowledgeCompletedAt: string | null
  actionStartedAt: string | null
  actionCompletedAt: string | null
}

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Map database row to response detail
 */
function mapToDetail(row: typeof buildTasks.$inferSelect): BuildTaskDetail {
  return {
    id: row.id,
    sourceId: row.sourceId,
    sourceUrl: row.sourceUrl,
    sourceName: row.sourceName,
    sourceCategory: row.sourceCategory,
    stage: row.stage,
    stageStatus: row.stageStatus,
    config: row.config ?? null,
    createdAt: row.createdAt.toISOString(),
    updatedAt: row.updatedAt.toISOString(),
    knowledgeStartedAt: row.knowledgeStartedAt?.toISOString() ?? null,
    knowledgeCompletedAt: row.knowledgeCompletedAt?.toISOString() ?? null,
    actionStartedAt: row.actionStartedAt?.toISOString() ?? null,
    actionCompletedAt: row.actionCompletedAt?.toISOString() ?? null,
  }
}

// ============================================================================
// GET /api/build-tasks/:id - Get build task by ID
// ============================================================================

export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ id: string }> }
) {
  try {
    const { id } = await params
    const taskId = parseInt(id, 10)

    if (isNaN(taskId)) {
      return invalidRequestResponse('Invalid task ID')
    }

    const db = getDb()

    const [task] = await db
      .select()
      .from(buildTasks)
      .where(eq(buildTasks.id, taskId))
      .limit(1)

    if (!task) {
      return notFoundResponse('Build task not found')
    }

    return successResponse(mapToDetail(task))
  } catch (error) {
    console.error('Get build task error:', error)
    return internalErrorResponse(
      error instanceof Error ? error.message : 'Failed to get build task'
    )
  }
}

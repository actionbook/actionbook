import { NextRequest } from 'next/server'
import { getDb, buildTasks } from '@actionbookdev/db'
import type {
  SourceCategory,
  BuildTaskStage,
  BuildTaskStageStatus,
  BuildTaskConfig,
} from '@actionbookdev/db'
import { desc, eq, and, type SQL } from 'drizzle-orm'
import {
  successResponse,
  invalidRequestResponse,
  internalErrorResponse,
} from '@/lib/response'

// ============================================================================
// Types
// ============================================================================

interface CreateBuildTaskRequest {
  sourceUrl: string
  sourceName?: string
  sourceCategory?: SourceCategory
  config?: BuildTaskConfig
}

interface BuildTaskItem {
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
 * Detect source category based on URL patterns
 *
 * - 'help': Help center / documentation sites (detected by URL patterns)
 * - 'any': General websites (processed by knowledge-builder-any)
 */
function detectCategory(url: URL): SourceCategory {
  const helpPatterns = [
    '/help',
    '/support',
    '/docs',
    '/faq',
    '/documentation',
    'help.',
    'support.',
    'docs.',
  ]
  return helpPatterns.some((p) => url.href.toLowerCase().includes(p))
    ? 'help'
    : 'any'
}

/**
 * Map database row to response item
 */
function mapToResponse(row: typeof buildTasks.$inferSelect): BuildTaskItem {
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
// POST /api/build-tasks - Create a new build task
// ============================================================================

export async function POST(request: NextRequest) {
  try {
    const body = (await request.json()) as CreateBuildTaskRequest

    // Validate sourceUrl
    if (!body.sourceUrl) {
      return invalidRequestResponse('sourceUrl is required')
    }

    // Validate URL format
    let url: URL
    try {
      url = new URL(body.sourceUrl)
    } catch {
      return invalidRequestResponse('Invalid URL format')
    }

    // Detect or use provided category
    const category: SourceCategory = body.sourceCategory ?? detectCategory(url)

    const db = getDb()

    // Create build task
    const [task] = await db
      .insert(buildTasks)
      .values({
        sourceUrl: body.sourceUrl,
        sourceName: body.sourceName ?? url.hostname,
        sourceCategory: category,
        stage: 'init',
        stageStatus: 'pending',
        config: body.config ?? {},
      })
      .returning()

    return successResponse(mapToResponse(task), 201)
  } catch (error) {
    console.error('Create build task error:', error)
    return internalErrorResponse(
      error instanceof Error ? error.message : 'Failed to create build task'
    )
  }
}

// ============================================================================
// GET /api/build-tasks - List build tasks
// ============================================================================

export async function GET(request: NextRequest) {
  try {
    const { searchParams } = new URL(request.url)

    // Parse query parameters
    const stage = searchParams.get('stage')
    const status = searchParams.get('status')
    const category = searchParams.get('category')
    const limit = Math.min(
      Math.max(parseInt(searchParams.get('limit') || '50', 10), 1),
      200
    )

    const db = getDb()

    // Build where conditions
    const conditions: SQL[] = []
    if (stage) {
      conditions.push(eq(buildTasks.stage, stage as BuildTaskStage))
    }
    if (status) {
      conditions.push(
        eq(buildTasks.stageStatus, status as BuildTaskStageStatus)
      )
    }
    if (category) {
      conditions.push(eq(buildTasks.sourceCategory, category as SourceCategory))
    }

    // Execute query
    const baseQuery = db.select().from(buildTasks)

    const results =
      conditions.length > 0
        ? await baseQuery
            .where(and(...conditions))
            .orderBy(desc(buildTasks.createdAt))
            .limit(limit)
        : await baseQuery.orderBy(desc(buildTasks.createdAt)).limit(limit)

    return successResponse({
      results: results.map(mapToResponse),
      count: results.length,
    })
  } catch (error) {
    console.error('List build tasks error:', error)
    return internalErrorResponse(
      error instanceof Error ? error.message : 'Failed to list build tasks'
    )
  }
}

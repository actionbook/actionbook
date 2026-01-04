/**
 * TaskGenerator Unit Tests - Build Task Integration
 * Tests the new buildTaskId-based uniqueness logic
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { TaskGenerator } from '../../src/task-worker/task-generator'
import {
  getDb,
  sources,
  buildTasks,
  recordingTasks,
  eq,
  inArray,
  sql,
  and,
  asc,
} from '@actionbookdev/db'
import type { Database } from '@actionbookdev/db'

let createdSourceIds: number[] = []
let createdBuildTaskIds: number[] = []

describe('TaskGenerator - Build Task Integration', () => {
  let generator: TaskGenerator
  let db: Database

  beforeEach(async () => {
    db = getDb()
    generator = new TaskGenerator(db)
    createdSourceIds = []
    createdBuildTaskIds = []
  })

  afterEach(async () => {
    // Clean up build_tasks (cascades to recording_tasks)
    if (createdBuildTaskIds.length > 0) {
      await db.delete(buildTasks).where(inArray(buildTasks.id, createdBuildTaskIds))
    }
    // Clean up sources (cascades to documents/chunks)
    if (createdSourceIds.length > 0) {
      await db.delete(sources).where(inArray(sources.id, createdSourceIds))
    }
  })

  // ========================================================================
  // UT-TG-BT-01: Generate tasks with buildTaskId
  // ========================================================================
  it('UT-TG-BT-01: Generate tasks with buildTaskId', async () => {
    // Setup: Create source, chunks, and build_task
    const sourceId = await createTestSource(db, 'airbnb.com')
    await createTestChunks(db, sourceId, 5, 'task_driven')
    const buildTaskId = await createTestBuildTask(db, sourceId)

    // Execute
    const count = await generator.generate(buildTaskId, sourceId, 10)

    // Assert
    expect(count).toBe(5)

    // Verify tasks have correct build_task_id
    const tasks = await db
      .select()
      .from(recordingTasks)
      .where(eq(recordingTasks.buildTaskId, buildTaskId))

    expect(tasks).toHaveLength(5)
    for (const task of tasks) {
      expect(task.buildTaskId).toBe(buildTaskId)
      expect(task.sourceId).toBe(sourceId)
    }
  })

  // ========================================================================
  // UT-TG-BT-02: Same chunk can be used in different build_tasks
  // ========================================================================
  it('UT-TG-BT-02: Same chunk can be used in different build_tasks', async () => {
    // Setup: Create 1 source with 3 chunks
    const sourceId = await createTestSource(db, 'airbnb.com')
    const chunkIds = await createTestChunks(db, sourceId, 3, 'task_driven')

    // Create 2 different build_tasks for the same source
    const buildTaskId1 = await createTestBuildTask(db, sourceId)
    const buildTaskId2 = await createTestBuildTask(db, sourceId)

    // Execute: Generate tasks for build_task 1
    const count1 = await generator.generate(buildTaskId1, sourceId, 10)
    expect(count1).toBe(3)

    // Execute: Generate tasks for build_task 2 (same chunks, different build_task)
    const count2 = await generator.generate(buildTaskId2, sourceId, 10)
    expect(count2).toBe(3) // Should create 3 new tasks

    // Verify: 6 total tasks (3 per build_task)
    const allTasks = await db
      .select()
      .from(recordingTasks)
      .where(eq(recordingTasks.sourceId, sourceId))

    expect(allTasks).toHaveLength(6)

    // Verify: Each build_task has 3 tasks
    const tasks1 = allTasks.filter((t) => t.buildTaskId === buildTaskId1)
    const tasks2 = allTasks.filter((t) => t.buildTaskId === buildTaskId2)

    expect(tasks1).toHaveLength(3)
    expect(tasks2).toHaveLength(3)

    // Verify: Both build_tasks reference the same chunks
    const chunkIds1 = tasks1.map((t) => t.chunkId).sort()
    const chunkIds2 = tasks2.map((t) => t.chunkId).sort()
    expect(chunkIds1).toEqual(chunkIds2)
  })

  // ========================================================================
  // UT-TG-BT-03: Deduplication within same build_task
  // ========================================================================
  it('UT-TG-BT-03: Deduplication within same build_task', async () => {
    // Setup
    const sourceId = await createTestSource(db, 'airbnb.com')
    await createTestChunks(db, sourceId, 5, 'task_driven')
    const buildTaskId = await createTestBuildTask(db, sourceId)

    // First generation
    const count1 = await generator.generate(buildTaskId, sourceId, 10)
    expect(count1).toBe(5)

    // Second generation (same build_task) - should create 0 tasks
    const count2 = await generator.generate(buildTaskId, sourceId, 10)
    expect(count2).toBe(0)

    // Verify: Still only 5 tasks
    const tasks = await db
      .select()
      .from(recordingTasks)
      .where(eq(recordingTasks.buildTaskId, buildTaskId))

    expect(tasks).toHaveLength(5)
  })

  // ========================================================================
  // UT-TG-BT-04: Partial generation preserves chunk-build_task uniqueness
  // ========================================================================
  it('UT-TG-BT-04: Partial generation preserves uniqueness', async () => {
    // Setup: 10 chunks, 2 build_tasks
    const sourceId = await createTestSource(db, 'airbnb.com')
    await createTestChunks(db, sourceId, 10, 'task_driven')
    const buildTaskId1 = await createTestBuildTask(db, sourceId)
    const buildTaskId2 = await createTestBuildTask(db, sourceId)

    // Build task 1: Generate 5 tasks (limit=5)
    const count1a = await generator.generate(buildTaskId1, sourceId, 5)
    expect(count1a).toBe(5)

    // Build task 2: Generate 3 tasks (limit=3)
    const count2a = await generator.generate(buildTaskId2, sourceId, 3)
    expect(count2a).toBe(3)

    // Build task 1: Generate remaining 5 tasks
    const count1b = await generator.generate(buildTaskId1, sourceId, 10)
    expect(count1b).toBe(5)

    // Build task 2: Generate remaining 7 tasks
    const count2b = await generator.generate(buildTaskId2, sourceId, 10)
    expect(count2b).toBe(7)

    // Verify totals
    const tasks1 = await db
      .select()
      .from(recordingTasks)
      .where(eq(recordingTasks.buildTaskId, buildTaskId1))

    const tasks2 = await db
      .select()
      .from(recordingTasks)
      .where(eq(recordingTasks.buildTaskId, buildTaskId2))

    expect(tasks1).toHaveLength(10)
    expect(tasks2).toHaveLength(10)
  })

  // ========================================================================
  // UT-TG-BT-05: Verify unique constraint prevents duplicates
  // ========================================================================
  it('UT-TG-BT-05: Unique constraint prevents duplicates', async () => {
    // Setup
    const sourceId = await createTestSource(db, 'airbnb.com')
    const chunkIds = await createTestChunks(db, sourceId, 1, 'task_driven')
    const buildTaskId = await createTestBuildTask(db, sourceId)
    const chunkId = chunkIds[0]

    // Create first recording_task
    await db.execute(sql`
      INSERT INTO recording_tasks (source_id, build_task_id, chunk_id, start_url, status)
      VALUES (${sourceId}, ${buildTaskId}, ${chunkId}, 'https://example.com', 'pending')
    `)

    // Try to create duplicate - should fail with unique constraint violation
    await expect(async () => {
      await db.execute(sql`
        INSERT INTO recording_tasks (source_id, build_task_id, chunk_id, start_url, status)
        VALUES (${sourceId}, ${buildTaskId}, ${chunkId}, 'https://example.com', 'pending')
      `)
    }).rejects.toThrow()

    // Verify only 1 task exists
    const tasks = await db
      .select()
      .from(recordingTasks)
      .where(
        and(
          eq(recordingTasks.buildTaskId, buildTaskId),
          eq(recordingTasks.chunkId, chunkId)
        )
      )

    expect(tasks).toHaveLength(1)
  })

  // ========================================================================
  // UT-TG-BT-06: Reset recording_tasks for re-execution
  // ========================================================================
  it('UT-TG-BT-06: Reset recording_tasks for re-execution', async () => {
    // Setup
    const sourceId = await createTestSource(db, 'airbnb.com')
    await createTestChunks(db, sourceId, 3, 'task_driven')
    const buildTaskId = await createTestBuildTask(db, sourceId)

    // Generate initial tasks
    const count1 = await generator.generate(buildTaskId, sourceId, 10)
    expect(count1).toBe(3)

    // Mark some tasks as completed and failed
    const tasks = await db
      .select()
      .from(recordingTasks)
      .where(eq(recordingTasks.buildTaskId, buildTaskId))
      .orderBy(asc(recordingTasks.id))

    await db.execute(sql`
      UPDATE recording_tasks
      SET status = 'completed', completed_at = NOW()
      WHERE id = ${tasks[0].id}
    `)

    await db.execute(sql`
      UPDATE recording_tasks
      SET status = 'failed', error_message = 'Test error'
      WHERE id = ${tasks[1].id}
    `)

    // Verify statuses before reset
    const tasksBefore = await db
      .select()
      .from(recordingTasks)
      .where(eq(recordingTasks.buildTaskId, buildTaskId))
      .orderBy(asc(recordingTasks.id))

    expect(tasksBefore[0].status).toBe('completed')
    expect(tasksBefore[1].status).toBe('failed')
    expect(tasksBefore[2].status).toBe('pending')

    // Reset tasks
    const { TaskScheduler } = await import('../../src/task-worker/task-scheduler')
    const taskScheduler = new TaskScheduler(db)
    const resetCount = await taskScheduler.resetRecordingTasksForBuildTask(
      buildTaskId
    )

    expect(resetCount).toBe(3) // All 3 tasks should be reset

    // Verify all tasks are now pending with cleared data
    const tasksAfter = await db
      .select()
      .from(recordingTasks)
      .where(eq(recordingTasks.buildTaskId, buildTaskId))
      .orderBy(asc(recordingTasks.id))

    for (const task of tasksAfter) {
      expect(task.status).toBe('pending')
      expect(task.completedAt).toBeNull()
      expect(task.errorMessage).toBeNull()
      expect(task.progress).toBe(0)
      expect(task.elementsDiscovered).toBe(0)
      expect(task.attemptCount).toBe(0)
      expect(task.tokensUsed).toBe(0)
    }
  })

  // ========================================================================
  // UT-TG-BT-07: Reset skips running tasks
  // ========================================================================
  it('UT-TG-BT-07: Reset skips running tasks', async () => {
    // Setup
    const sourceId = await createTestSource(db, 'airbnb.com')
    await createTestChunks(db, sourceId, 3, 'task_driven')
    const buildTaskId = await createTestBuildTask(db, sourceId)

    // Generate initial tasks
    await generator.generate(buildTaskId, sourceId, 10)

    const tasks = await db
      .select()
      .from(recordingTasks)
      .where(eq(recordingTasks.buildTaskId, buildTaskId))
      .orderBy(asc(recordingTasks.id))

    // Mark first task as running
    await db.execute(sql`
      UPDATE recording_tasks
      SET status = 'running', started_at = NOW()
      WHERE id = ${tasks[0].id}
    `)

    // Mark second task as completed
    await db.execute(sql`
      UPDATE recording_tasks
      SET status = 'completed', completed_at = NOW()
      WHERE id = ${tasks[1].id}
    `)

    // Reset tasks
    const { TaskScheduler } = await import('../../src/task-worker/task-scheduler')
    const taskScheduler = new TaskScheduler(db)
    const resetCount = await taskScheduler.resetRecordingTasksForBuildTask(
      buildTaskId
    )

    // Should reset 2 tasks (completed + pending), skip running
    expect(resetCount).toBe(2)

    // Verify statuses
    const tasksAfter = await db
      .select()
      .from(recordingTasks)
      .where(eq(recordingTasks.buildTaskId, buildTaskId))
      .orderBy(asc(recordingTasks.id))

    expect(tasksAfter[0].status).toBe('running') // Still running
    expect(tasksAfter[1].status).toBe('pending') // Reset from completed
    expect(tasksAfter[2].status).toBe('pending') // Already pending
  })
})

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Create a test source
 */
async function createTestSource(db: Database, domain: string): Promise<number> {
  const timestamp = Date.now()
  const random = Math.floor(Math.random() * 10000)
  const uniqueDomain = `ut-bt-${timestamp}-${random}.${domain}`
  const uniqueName = `${uniqueDomain}_${timestamp}_${random}`

  const result = await db
    .insert(sources)
    .values({
      name: uniqueName,
      baseUrl: `https://${uniqueDomain}`,
      description: `Test source: ${domain}`,
      domain: uniqueDomain,
      crawlConfig: {},
    })
    .returning({ id: sources.id })

  const sourceId = result[0].id
  createdSourceIds.push(sourceId)
  return sourceId
}

/**
 * Create test chunks
 */
async function createTestChunks(
  db: Database,
  sourceId: number,
  count: number,
  contentType: 'task_driven' | 'exploratory'
): Promise<number[]> {
  const chunkIds: number[] = []

  for (let i = 0; i < count; i++) {
    const timestamp = Date.now()
    const uniqueHash = `bt_hash_${sourceId}_${timestamp}_${i}_${Math.random()}`

    // Create document
    const docResult = await db.execute<{ id: number }>(sql`
      INSERT INTO documents (source_id, url, url_hash, title, content_text)
      VALUES (
        ${sourceId},
        ${`https://example.com/bt_page${sourceId}_${i}`},
        ${uniqueHash},
        ${`BT Test Page ${i}`},
        ${'Test content'}
      )
      RETURNING id
    `)

    const documentId = docResult.rows[0].id

    // Create chunk
    const content =
      contentType === 'task_driven'
        ? `Task: Search\nSteps:\n1. Click`
        : `# Page\n- Nav`

    const contentHash = `bt_chunkhash_${sourceId}_${timestamp}_${i}_${Math.random()}`
    const result = await db.execute<{ id: number }>(sql`
      INSERT INTO chunks (document_id, content, content_hash, chunk_index, start_char, end_char, token_count)
      VALUES (${documentId}, ${content}, ${contentHash}, ${i}, 0, ${content.length}, 50)
      RETURNING id
    `)

    chunkIds.push(result.rows[0].id)
  }

  return chunkIds
}

/**
 * Create a test build_task
 */
async function createTestBuildTask(
  db: Database,
  sourceId: number
): Promise<number> {
  const timestamp = Date.now()
  const random = Math.floor(Math.random() * 10000)

  const result = await db
    .insert(buildTasks)
    .values({
      sourceId,
      sourceUrl: `https://test-${timestamp}-${random}.com`,
      sourceName: `test-${timestamp}-${random}`,
      sourceCategory: 'any',
      stage: 'knowledge_build',
      stageStatus: 'completed',
      config: {},
    })
    .returning({ id: buildTasks.id })

  const buildTaskId = result[0].id
  createdBuildTaskIds.push(buildTaskId)
  return buildTaskId
}

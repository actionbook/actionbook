/**
 * TaskGenerator Unit Tests
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { TaskGenerator } from '../../src/task-worker/task-generator'
import {
  getDb,
  sources,
  documents,
  buildTasks,
  recordingTasks,
  eq,
  inArray,
  sql,
} from '@actionbookdev/db'
import type { Database } from '@actionbookdev/db'

let createdSourceIds: number[] = []
let createdBuildTaskIds: number[] = []

describe('TaskGenerator', () => {
  let generator: TaskGenerator
  let db: Database

  beforeEach(async () => {
    db = getDb()
    generator = new TaskGenerator(db)
    createdSourceIds = []
    createdBuildTaskIds = []
  })

  afterEach(async () => {
    // Clean up build_tasks first (cascades to recording_tasks)
    if (createdBuildTaskIds.length > 0) {
      await db.delete(buildTasks).where(inArray(buildTasks.id, createdBuildTaskIds))
    }
    // Then clean up sources (cascades to documents/chunks)
    if (createdSourceIds.length > 0) {
      await db.delete(sources).where(inArray(sources.id, createdSourceIds))
    }
  })

  // ========================================================================
  // UT-TG-01: Generate tasks (no filter)
  // ========================================================================
  it('UT-TG-01: Generate tasks (no filter)', async () => {
    // Setup: Create test data - 15 chunks
    const sourceId = await createTestSource(db, 'airbnb.com')
    await createTestChunks(db, sourceId, 15, 'task_driven')
    const buildTaskId = await createTestBuildTask(db, sourceId)

    // Execute
    const count = await generator.generate(buildTaskId, sourceId)

    // Assert
    expect(count).toBe(10) // Even with 15 records, only generates 10

    // Verify tasks created
    const tasks = await db
      .select()
      .from(recordingTasks)
      .where(eq(recordingTasks.sourceId, sourceId))

    expect(tasks).toHaveLength(10)
  })

  // ========================================================================
  // UT-TG-02: Generate tasks (specify sourceId)
  // ========================================================================
  it('UT-TG-02: Generate tasks (specify sourceId)', async () => {
    // Setup: Create 2 sources with chunks
    const sourceId1 = await createTestSource(db, 'airbnb.com')
    const sourceId2 = await createTestSource(db, 'booking.com')

    await createTestChunks(db, sourceId1, 8, 'task_driven')
    const buildTaskId1 = await createTestBuildTask(db, sourceId1)
    await createTestChunks(db, sourceId2, 8, 'exploratory')
    const buildTaskId2 = await createTestBuildTask(db, sourceId2)

    // Execute: Generate only for sourceId1
    const count = await generator.generate(buildTaskId1, sourceId1)

    // Assert
    expect(count).toBe(8)

    // Verify only sourceId1 tasks created
    const tasks1 = await db
      .select()
      .from(recordingTasks)
      .where(eq(recordingTasks.sourceId, sourceId1))

    const tasks2 = await db
      .select()
      .from(recordingTasks)
      .where(eq(recordingTasks.sourceId, sourceId2))

    expect(tasks1).toHaveLength(8)
    expect(tasks2).toHaveLength(0) // sourceId2 has no tasks generated
  })

  // ========================================================================
  // UT-TG-03: chunk_type correct detection
  // ========================================================================
  it('UT-TG-03: chunk_type correct detection', async () => {
    // Setup: Create chunks with different content types
    const sourceId = await createTestSource(db, 'airbnb.com')

    // 3 task-driven chunks
    await createTestChunks(db, sourceId, 3, 'task_driven')
    const buildTaskId = await createTestBuildTask(db, sourceId)

    // 5 exploratory chunks
    await createTestChunks(db, sourceId, 5, 'exploratory')

    // Execute
    const count = await generator.generate(buildTaskId, sourceId)

    // Assert
    expect(count).toBe(8)

    // Verify both chunk types exist
    const tasks = await db
      .select()
      .from(recordingTasks)
      .where(eq(recordingTasks.sourceId, sourceId))

    const taskDrivenCount = tasks.filter(
      (t) => (t.config as any)?.chunk_type === 'task_driven'
    ).length

    const exploratoryCount = tasks.filter(
      (t) => (t.config as any)?.chunk_type === 'exploratory'
    ).length

    expect(taskDrivenCount).toBe(3)
    expect(exploratoryCount).toBe(5)
  })

  // ========================================================================
  // UT-TG-04: Source with no chunks
  // ========================================================================
  it('UT-TG-04: Source with no chunks', async () => {
    // Setup: Create a source with no documents/chunks
    const sourceId = await createTestSource(db, 'empty-source.test')
    const buildTaskId = await createTestBuildTask(db, sourceId)

    // Execute: Generate tasks for that source
    const count = await generator.generate(buildTaskId, sourceId)

    // Assert
    expect(count).toBe(0)

    // Verify no tasks created for that source
    const tasks = await db
      .select()
      .from(recordingTasks)
      .where(eq(recordingTasks.sourceId, sourceId))
    expect(tasks).toHaveLength(0)
  })

  // ========================================================================
  // UT-TG-05: chunk_id association correct
  // ========================================================================
  it('UT-TG-05: chunk_id association correct', async () => {
    // Setup
    const sourceId = await createTestSource(db, 'airbnb.com')
    const chunkIds = await createTestChunks(db, sourceId, 5, 'task_driven')
    const buildTaskId = await createTestBuildTask(db, sourceId)

    // Execute
    const count = await generator.generate(buildTaskId, sourceId)

    // Assert
    expect(count).toBe(5)

    // Verify chunk_id mappings
    const tasks = await db
      .select()
      .from(recordingTasks)
      .where(eq(recordingTasks.sourceId, sourceId))

    for (const task of tasks) {
      expect(task.chunkId).not.toBeNull()
      expect(chunkIds).toContain(task.chunkId)
    }
  })

  // ========================================================================
  // UT-TG-06: Initial state correct
  // ========================================================================
  it('UT-TG-06: Initial state correct', async () => {
    // Setup
    const sourceId = await createTestSource(db, 'airbnb.com')
    await createTestChunks(db, sourceId, 3, 'task_driven')
    const buildTaskId = await createTestBuildTask(db, sourceId)

    // Execute
    await generator.generate(buildTaskId, sourceId)

    // Assert
    const tasks = await db
      .select()
      .from(recordingTasks)
      .where(eq(recordingTasks.sourceId, sourceId))

    for (const task of tasks) {
      expect(task.status).toBe('pending')
      expect(task.attemptCount).toBe(0)
      expect(task.createdAt).toBeDefined()
      expect(task.updatedAt).toBeDefined()
    }
  })

  // ========================================================================
  // UT-TG-07: config field correct
  // ========================================================================
  it('UT-TG-07: config field correct', async () => {
    // Setup
    const sourceId = await createTestSource(db, 'airbnb.com')
    await createTestChunks(db, sourceId, 2, 'exploratory')
    const buildTaskId = await createTestBuildTask(db, sourceId)

    // Execute
    await generator.generate(buildTaskId, sourceId)

    // Assert
    const tasks = await db
      .select()
      .from(recordingTasks)
      .where(eq(recordingTasks.sourceId, sourceId))

    for (const task of tasks) {
      expect(task.config).toBeDefined()
      expect((task.config as any).chunk_type).toBe('exploratory')
    }
  })

  // ========================================================================
  // UT-TG-08: LIMIT 10 restriction
  // ========================================================================
  it('UT-TG-08: LIMIT 10 restriction', async () => {
    // Setup: Create 100 chunks
    const sourceId = await createTestSource(db, 'airbnb.com')
    await createTestChunks(db, sourceId, 100, 'task_driven')
    const buildTaskId = await createTestBuildTask(db, sourceId)

    // Execute
    const count = await generator.generate(buildTaskId, sourceId)

    // Assert: Only 10 tasks created
    expect(count).toBe(10)

    const tasks = await db
      .select()
      .from(recordingTasks)
      .where(eq(recordingTasks.sourceId, sourceId))

    expect(tasks).toHaveLength(10)
  })

  // ========================================================================
  // UT-TG-09: Skip chunks that already have recording_tasks (deduplication)
  // ========================================================================
  it('UT-TG-09: Skip chunks that already have recording_tasks (deduplication)', async () => {
    // Setup: Create 10 chunks
    const sourceId = await createTestSource(db, 'airbnb.com')
    await createTestChunks(db, sourceId, 10, 'task_driven')
    const buildTaskId = await createTestBuildTask(db, sourceId)

    // First generation: creates 10 tasks
    const count1 = await generator.generate(buildTaskId, sourceId, 100)
    expect(count1).toBe(10)

    // Verify 10 tasks created
    const tasks1 = await db
      .select()
      .from(recordingTasks)
      .where(eq(recordingTasks.sourceId, sourceId))
    expect(tasks1).toHaveLength(10)

    // Second generation: should create 0 tasks (all chunks already have tasks)
    const count2 = await generator.generate(buildTaskId, sourceId, 100)
    expect(count2).toBe(0)

    // Verify still only 10 tasks (no duplicates)
    const tasks2 = await db
      .select()
      .from(recordingTasks)
      .where(eq(recordingTasks.sourceId, sourceId))
    expect(tasks2).toHaveLength(10)
  })

  // ========================================================================
  // UT-TG-10: Only generate tasks for chunks without existing tasks
  // ========================================================================
  it('UT-TG-10: Only generate tasks for chunks without existing tasks', async () => {
    // Setup: Create 10 chunks
    const sourceId = await createTestSource(db, 'airbnb.com')
    await createTestChunks(db, sourceId, 10, 'task_driven')
    const buildTaskId = await createTestBuildTask(db, sourceId)

    // First generation: creates 5 tasks (limit=5)
    const count1 = await generator.generate(buildTaskId, sourceId, 5)
    expect(count1).toBe(5)

    // Second generation: creates 5 more tasks (remaining chunks)
    const count2 = await generator.generate(buildTaskId, sourceId, 10)
    expect(count2).toBe(5)

    // Third generation: creates 0 tasks (all chunks have tasks)
    const count3 = await generator.generate(buildTaskId, sourceId, 10)
    expect(count3).toBe(0)

    // Verify total 10 tasks
    const tasks = await db
      .select()
      .from(recordingTasks)
      .where(eq(recordingTasks.sourceId, sourceId))
    expect(tasks).toHaveLength(10)
  })
})

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Create a test source
 */
async function createTestSource(db: Database, domain: string): Promise<number> {
  // Generate unique name to avoid conflicts
  const timestamp = Date.now()
  const random = Math.floor(Math.random() * 10000)
  const uniqueDomain = `ut-${timestamp}-${random}.${domain}`
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
 * Create test chunks with specified content type
 */
async function createTestChunks(
  db: Database,
  sourceId: number,
  count: number,
  contentType: 'task_driven' | 'exploratory'
): Promise<number[]> {
  const chunkIds: number[] = []

  for (let i = 0; i < count; i++) {
    // Create document with unique hash
    const timestamp = Date.now()
    const uniqueHash = `hash_${sourceId}_${timestamp}_${i}`

    // Use raw SQL to avoid schema drift (some DBs may not have documents.elements column).
    const docResult = await db.execute<{ id: number }>(sql`
      INSERT INTO documents (source_id, url, url_hash, title, content_text)
      VALUES (
        ${sourceId},
        ${`https://example.com/page${sourceId}_${i}`},
        ${uniqueHash},
        ${`Test Page ${i}`},
        ${'Test content'}
      )
      RETURNING id
    `)

    const documentId = docResult.rows[0].id

    // Create chunk with appropriate content
    const content =
      contentType === 'task_driven'
        ? `Task: Search for hotels\nSteps:\n1. Click search\n2. Type location`
        : `# Homepage\n- Navigation bar\n- Content area`

    const contentHash = `chunkhash_${sourceId}_${timestamp}_${i}`
    // Use raw SQL to avoid schema drift (some local DBs may not have newer columns like chunks.elements)
    const result = await db.execute<{ id: number }>(sql`
      INSERT INTO chunks (document_id, content, content_hash, chunk_index, start_char, end_char, token_count)
      VALUES (${documentId}, ${content}, ${contentHash}, ${i}, 0, ${content.length}, 100)
      RETURNING id
    `)

    chunkIds.push(result.rows[0].id)
  }

  return chunkIds
}

/**
 * Create a default test build_task for a source
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

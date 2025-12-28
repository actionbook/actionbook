/**
 * TaskQuery Unit Tests (M1: Query interface for verifying M1 data)
 *
 * Note: These tests use real database connection since mocking Drizzle ORM's
 * complex chain patterns (select/from/where/join/orderBy/limit) is error-prone.
 * The tests validate the TaskQuery class against the actual database schema.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import {
  TaskQuery,
  type TaskStats,
  type TaskDetail,
} from '../../src/task-worker/task-query'
import {
  getDb,
  sources,
  documents,
  chunks,
  recordingTasks,
  eq,
  sql,
} from '@actionbookdev/db'
import type { Database } from '@actionbookdev/db'

describe('TaskQuery', () => {
  let db: Database
  let query: TaskQuery
  let testSourceId: number
  let testDocId: number
  let chunk1Id: number
  let chunk2Id: number

  beforeAll(async () => {
    db = getDb()
    query = new TaskQuery(db)

    const timestamp = Date.now()
    const uniqueId = Math.random().toString(36).substring(7)

    // Create test data
    const [source] = await db
      .insert(sources)
      .values({
        domain: `taskquery-test-${uniqueId}.com`,
        name: `TaskQuery Test Site ${uniqueId}`,
        baseUrl: `https://taskquery-test-${uniqueId}.com`,
        crawlConfig: {},
      })
      .returning()
    testSourceId = source.id

    // Create test document
    const docInsert = await db.execute<{ id: number }>(sql`
      INSERT INTO documents (source_id, url, url_hash, title, content_text)
      VALUES (
        ${testSourceId},
        ${`https://taskquery-test-${uniqueId}.com/`},
        ${`urlhash_${timestamp}_${uniqueId}`},
        ${'TaskQuery Test Page'},
        ${'Test content'}
      )
      RETURNING id
    `)
    testDocId = docInsert.rows[0].id

    // Create test chunks
    // Use raw SQL insert to avoid schema drift (older DB may not have `chunks.elements` column).
    const chunk1Hash = `chunkhash_${timestamp}_${uniqueId}_1`
    const chunk2Hash = `chunkhash_${timestamp}_${uniqueId}_2`

    const chunk1Result = await db.execute<{ id: number }>(sql`
      INSERT INTO chunks (document_id, content, content_hash, chunk_index, start_char, end_char, token_count)
      VALUES (${testDocId}, ${'Task: Test action 1'}, ${chunk1Hash}, 0, 0, 20, 10)
      RETURNING id
    `)
    chunk1Id = chunk1Result.rows[0].id

    const chunk2Result = await db.execute<{ id: number }>(sql`
      INSERT INTO chunks (document_id, content, content_hash, chunk_index, start_char, end_char, token_count)
      VALUES (${testDocId}, ${'# Test Heading\nExploratory content'}, ${chunk2Hash}, 1, 0, 35, 15)
      RETURNING id
    `)
    chunk2Id = chunk2Result.rows[0].id

    // Create test tasks
    const startUrl = `https://taskquery-test-${uniqueId}.com/`
    await db.insert(recordingTasks).values([
      {
        sourceId: testSourceId,
        scenario: `ut-${uniqueId}`,
        chunkId: chunk1Id,
        status: 'completed',
        progress: 100,
        startUrl,
        config: { chunk_type: 'task_driven' },
        attemptCount: 1,
      },
      {
        sourceId: testSourceId,
        scenario: `ut-${uniqueId}`,
        chunkId: chunk2Id,
        status: 'pending',
        progress: 0,
        startUrl,
        config: { chunk_type: 'exploratory' },
        attemptCount: 0,
      },
      {
        sourceId: testSourceId,
        scenario: `ut-${uniqueId}`,
        chunkId: chunk1Id,
        status: 'failed',
        progress: 50,
        startUrl,
        config: { chunk_type: 'task_driven' },
        attemptCount: 3,
        errorMessage: 'Timeout error',
      },
    ])
  })

  afterAll(async () => {
    // Cleanup test data (DB schema drifts across environments; deleting the source cascades).
    await db.delete(sources).where(eq(sources.id, testSourceId))
  })

  // ========================================================================
  // UT-TQ-01: getTaskStats - Get task statistics
  // ========================================================================
  it('UT-TQ-01: getTaskStats returns correct statistics', async () => {
    const stats = await query.getTaskStats(testSourceId)

    expect(stats).not.toBeNull()
    expect(stats!.sourceId).toBe(testSourceId)
    expect(stats!.sourceDomain).toContain('taskquery-test')
    expect(stats!.totalTasks).toBe(3)
    expect(stats!.pendingTasks).toBe(1)
    expect(stats!.runningTasks).toBe(0)
    expect(stats!.completedTasks).toBe(1)
    expect(stats!.failedTasks).toBe(1)
    expect(stats!.taskDrivenCount).toBe(2)
    expect(stats!.exploratoryCount).toBe(1)
  })

  it('UT-TQ-02: getTaskStats returns null for non-existent source', async () => {
    const stats = await query.getTaskStats(999999)
    expect(stats).toBeNull()
  })

  // ========================================================================
  // UT-TQ-03: getTaskDetails - Get task details
  // ========================================================================
  it('UT-TQ-03: getTaskDetails returns all task details', async () => {
    const details = await query.getTaskDetails(testSourceId)

    expect(details).toHaveLength(3)

    // All details should have required fields
    for (const detail of details) {
      expect(detail.taskId).toBeDefined()
      expect(detail.status).toBeDefined()
      expect(detail.startUrl).toContain('taskquery-test')
      expect(['task_driven', 'exploratory']).toContain(detail.chunkType)
    }

    // Check for completed task
    const completedTask = details.find((d) => d.status === 'completed')
    expect(completedTask).toBeDefined()
    expect(completedTask!.progress).toBe(100)

    // Check for failed task
    const failedTask = details.find((d) => d.status === 'failed')
    expect(failedTask).toBeDefined()
    expect(failedTask!.errorMessage).toBe('Timeout error')
    expect(failedTask!.attemptCount).toBe(3)
  })

  it('UT-TQ-04: getTaskDetails returns empty array for source with no tasks', async () => {
    // Create a source without tasks
    const uniqueId = Math.random().toString(36).substring(7)
    const [emptySource] = await db
      .insert(sources)
      .values({
        domain: `empty-source-${uniqueId}.com`,
        name: `Empty Source ${uniqueId}`,
        baseUrl: `https://empty-source-${uniqueId}.com`,
        crawlConfig: {},
      })
      .returning()

    try {
      const details = await query.getTaskDetails(emptySource.id)
      expect(details).toEqual([])
    } finally {
      await db.delete(sources).where(eq(sources.id, emptySource.id))
    }
  })

  // ========================================================================
  // UT-TQ-05: searchTasks - Keyword search
  // ========================================================================
  it('UT-TQ-05: searchTasks filters by keyword', async () => {
    const results = await query.searchTasks('taskquery-test')

    expect(results.length).toBeGreaterThan(0)
    for (const result of results) {
      const hasKeyword =
        result.documentTitle?.toLowerCase().includes('taskquery') ||
        result.documentUrl?.toLowerCase().includes('taskquery') ||
        result.startUrl?.toLowerCase().includes('taskquery')
      expect(hasKeyword).toBe(true)
    }
  })

  it('UT-TQ-06: searchTasks is case-insensitive', async () => {
    const resultsLower = await query.searchTasks('taskquery-test')
    const resultsUpper = await query.searchTasks('TASKQUERY-TEST')

    expect(resultsLower.length).toBe(resultsUpper.length)
  })

  it('UT-TQ-07: searchTasks returns empty for non-matching keyword', async () => {
    const results = await query.searchTasks('nonexistent-xyz-12345')
    expect(results).toEqual([])
  })

  // ========================================================================
  // UT-TQ-08: getAllSourcesStats - Get all sources statistics
  // ========================================================================
  it('UT-TQ-08: getAllSourcesStats includes test source', async () => {
    const allStats = await query.getAllSourcesStats()

    // Should include our test source
    const testStats = allStats.find((s) => s.sourceId === testSourceId)
    expect(testStats).toBeDefined()
    expect(testStats!.totalTasks).toBe(3)
  })

  // ========================================================================
  // Edge cases
  // ========================================================================
  it('UT-TQ-09: TaskDetail includes joined document data', async () => {
    const details = await query.getTaskDetails(testSourceId)
    const detailWithDoc = details.find((d) => d.documentTitle !== null)

    expect(detailWithDoc).toBeDefined()
    expect(detailWithDoc!.documentTitle).toBe('TaskQuery Test Page')
    expect(detailWithDoc!.documentUrl).toContain('taskquery-test')
  })
})

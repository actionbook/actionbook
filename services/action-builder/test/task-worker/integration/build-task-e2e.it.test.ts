/**
 * IT-401: BuildTaskWorker End-to-End Integration Test
 *
 * Tests the complete build task workflow from picking up a task to completion.
 * Uses real database but mocks ActionBuilder to avoid browser/LLM calls.
 */

import { describe, it, expect, beforeAll, afterAll, vi } from 'vitest'
import { BuildTaskWorker } from '../../../src/task-worker/build-task-worker'
import { BuildTaskScheduler } from '../../../src/task-worker/build-task-scheduler'
import {
  getDb,
  sources,
  documents,
  buildTasks,
  recordingTasks,
  eq,
  sql,
} from '@actionbookdev/db'
import type { Database } from '@actionbookdev/db'
import type { BuildTaskWorkerConfig } from '../../../src/task-worker/types'

// Mock ActionBuilder to avoid real browser and LLM calls
vi.mock('../../../src/ActionBuilder', () => ({
  ActionBuilder: vi.fn().mockImplementation(() => ({
    initialize: vi.fn().mockResolvedValue(undefined),
    build: vi.fn().mockResolvedValue({
      success: true,
      turns: 5,
      totalDuration: 10000,
      totalTokens: 1500,
      savedPath: './output/test.yaml',
      siteCapability: {
        domain: 'example.com',
        pages: {
          home: {
            elements: {
              test_element: {},
            },
          },
        },
        global_elements: {},
      },
    }),
    close: vi.fn().mockResolvedValue(undefined),
  })),
}))

describe('IT-401: BuildTaskWorker End-to-End Integration Test', () => {
  let db: Database
  let worker: BuildTaskWorker
  let scheduler: BuildTaskScheduler
  let testSourceId: number
  let testBuildTaskId: number
  const testRunId = `it-401-${Date.now()}-${Math.floor(Math.random() * 10000)}`
  const testDomain = `${testRunId}.example.test`

  // Mock config for BuildTaskWorker
  const mockConfig: BuildTaskWorkerConfig = {
    llmApiKey: 'test-api-key',
    llmBaseURL: 'https://api.test.com/v1',
    llmModel: 'test-model',
    databaseUrl: 'postgres://test:test@localhost:5432/test',
    headless: true,
    maxTurns: 30,
    outputDir: './output',
    recordingTaskLimit: 10,
  }

  beforeAll(async () => {
    // Initialize database
    db = getDb()
    scheduler = new BuildTaskScheduler(db)
    worker = new BuildTaskWorker(db, mockConfig)

    // Create test data: source + documents + chunks + build_task
    testSourceId = await createTestSource(db, testDomain)
    await createTestDocumentsAndChunks(db, testSourceId, testDomain, 5)
    testBuildTaskId = await createTestBuildTask(db, testSourceId, testDomain)
  })

  afterAll(async () => {
    // Clean up test data
    // Delete build_task first (no FK constraint)
    await db.delete(buildTasks).where(eq(buildTasks.id, testBuildTaskId))
    // Delete recording_tasks (FK to source)
    await db
      .delete(recordingTasks)
      .where(eq(recordingTasks.sourceId, testSourceId))
    // Delete source (will cascade to documents/chunks)
    await db.delete(sources).where(eq(sources.id, testSourceId))
  })

  // IT-401-01: Complete workflow test
  it('should complete full build task workflow', async () => {
    // Verify initial state
    const [initialTask] = await db
      .select()
      .from(buildTasks)
      .where(eq(buildTasks.id, testBuildTaskId))

    expect(initialTask.stage).toBe('knowledge_build')
    expect(initialTask.stageStatus).toBe('completed')
    expect(initialTask.sourceId).toBe(testSourceId)

    // Run worker
    const result = await worker.runOnce()

    // Verify result
    expect(result).not.toBeNull()
    expect(result?.success).toBe(true)
    expect(result?.taskId).toBe(testBuildTaskId)
    expect(result?.recordingTasksCreated).toBeGreaterThan(0)

    // Verify build_task state after completion
    const [completedTask] = await db
      .select()
      .from(buildTasks)
      .where(eq(buildTasks.id, testBuildTaskId))

    expect(completedTask.stage).toBe('completed')
    expect(completedTask.stageStatus).toBe('completed')
    expect(completedTask.actionStartedAt).not.toBeNull()
    expect(completedTask.actionCompletedAt).not.toBeNull()

    // Verify recording_tasks were created
    const createdRecordingTasks = await db
      .select()
      .from(recordingTasks)
      .where(eq(recordingTasks.sourceId, testSourceId))

    expect(createdRecordingTasks.length).toBeGreaterThan(0)

    console.log('IT-401-01 Complete workflow test passed:')
    console.log(`  - Build task ID: ${testBuildTaskId}`)
    console.log(`  - Recording tasks created: ${result?.recordingTasksCreated}`)
    console.log(
      `  - Recording tasks completed: ${result?.recordingTasksCompleted}`
    )
    console.log(`  - Duration: ${result?.duration_ms}ms`)
  }, 60000)

  // IT-401-02: Returns null when no tasks available
  it('should return null when no tasks available', async () => {
    // The previous test completed the build_task, so no tasks should be available
    const result = await worker.runOnce()
    expect(result).toBeNull()
  })
})

describe('IT-401: BuildTaskScheduler Retry Mechanism Test', () => {
  let db: Database
  let scheduler: BuildTaskScheduler
  let testBuildTaskId: number
  const testRunId = `it-401-retry-${Date.now()}-${Math.floor(
    Math.random() * 10000
  )}`
  const testDomain = `${testRunId}.example.test`

  beforeAll(async () => {
    db = getDb()
    scheduler = new BuildTaskScheduler(db, { maxAttempts: 3 })

    // Create test build_task for retry testing
    testBuildTaskId = await createTestBuildTaskForRetry(db, testDomain)
  })

  afterAll(async () => {
    // Clean up
    await db.delete(buildTasks).where(eq(buildTasks.id, testBuildTaskId))
  })

  // IT-401-03: Retry mechanism test via scheduler directly
  it('should mark task as error after max attempts via scheduler', async () => {
    // Verify initial state
    let [task] = await db
      .select()
      .from(buildTasks)
      .where(eq(buildTasks.id, testBuildTaskId))
    expect(task.stage).toBe('knowledge_build')
    expect(task.stageStatus).toBe('completed')

    // First failure - should increment attemptCount but keep stage
    await scheduler.failTask(testBuildTaskId, 'Test error 1')

    ;[task] = await db
      .select()
      .from(buildTasks)
      .where(eq(buildTasks.id, testBuildTaskId))
    expect(task.stage).toBe('knowledge_build') // Still ready for retry
    expect(task.stageStatus).toBe('completed') // Keep completed for retry
    expect((task.config as any)?.attemptCount).toBe(1)

    // Second failure
    await scheduler.failTask(testBuildTaskId, 'Test error 2')

    ;[task] = await db
      .select()
      .from(buildTasks)
      .where(eq(buildTasks.id, testBuildTaskId))
    expect(task.stage).toBe('knowledge_build')
    expect((task.config as any)?.attemptCount).toBe(2)

    // Third failure - should mark as error (maxAttempts = 3)
    await scheduler.failTask(testBuildTaskId, 'Test error 3')

    ;[task] = await db
      .select()
      .from(buildTasks)
      .where(eq(buildTasks.id, testBuildTaskId))
    expect(task.stage).toBe('error')
    expect(task.stageStatus).toBe('error')
    expect((task.config as any)?.attemptCount).toBe(3)
    expect((task.config as any)?.lastError).toBe('Test error 3')

    console.log('IT-401-03 Retry mechanism test passed:')
    console.log(
      `  - Final attempt count: ${(task.config as any)?.attemptCount}`
    )
    console.log(`  - Final stage: ${task.stage}`)
  }, 30000)
})

/**
 * Helper: Create test source
 */
async function createTestSource(
  db: Database,
  testDomain: string
): Promise<number> {
  const timestamp = Date.now()
  const [result] = await db
    .insert(sources)
    .values({
      name: `integration_test_${timestamp}`,
      baseUrl: `https://${testDomain}`,
      description: 'IT-401 test source',
      domain: testDomain,
      crawlConfig: {},
    })
    .returning({ id: sources.id })

  return result.id
}

/**
 * Helper: Create test documents and chunks
 */
async function createTestDocumentsAndChunks(
  db: Database,
  sourceId: number,
  testDomain: string,
  count: number
): Promise<void> {
  const timestamp = Date.now()

  for (let i = 0; i < count; i++) {
    const isTaskDriven = i < count / 2
    const content = isTaskDriven
      ? `Task: Test task ${i}\nSteps:\n1. Step 1\n2. Step 2`
      : `# Page ${i}\n- Element A\n- Element B`

    // Create document
    const docResult = await db.execute<{ id: number }>(sql`
      INSERT INTO documents (source_id, url, url_hash, title, content_text)
      VALUES (
        ${sourceId},
        ${`https://${testDomain}/page${i}`},
        ${`hash_${timestamp}_${i}`},
        ${`Test Page ${i}`},
        ${content}
      )
      RETURNING id
    `)

    const documentId = docResult.rows[0].id

    // Create chunk
    await db.execute(sql`
      INSERT INTO chunks (document_id, content, content_hash, chunk_index, start_char, end_char, token_count)
      VALUES (${documentId}, ${content}, ${`chunkhash_${timestamp}_${i}`}, ${i}, 0, ${
      content.length
    }, 100)
    `)
  }
}

/**
 * Helper: Create test build_task in knowledge_build/completed state
 */
async function createTestBuildTask(
  db: Database,
  sourceId: number,
  testDomain: string
): Promise<number> {
  const [result] = await db
    .insert(buildTasks)
    .values({
      sourceId,
      sourceUrl: `https://${testDomain}`,
      sourceName: `IT-401 Test Site`,
      sourceCategory: 'help',
      stage: 'knowledge_build',
      stageStatus: 'completed',
      config: {},
      knowledgeStartedAt: new Date(),
      knowledgeCompletedAt: new Date(),
    })
    .returning({ id: buildTasks.id })

  return result.id
}

/**
 * Helper: Create test build_task for retry testing
 * Has a valid sourceId but will be used to test failTask() directly
 */
async function createTestBuildTaskForRetry(
  db: Database,
  testDomain: string
): Promise<number> {
  const [result] = await db
    .insert(buildTasks)
    .values({
      sourceId: null, // Null for retry test - scheduler won't pick it up directly
      sourceUrl: `https://${testDomain}`,
      sourceName: `IT-401 Retry Test Site`,
      sourceCategory: 'help',
      stage: 'knowledge_build',
      stageStatus: 'completed',
      config: {},
      knowledgeStartedAt: new Date(),
      knowledgeCompletedAt: new Date(),
    })
    .returning({ id: buildTasks.id })

  return result.id
}

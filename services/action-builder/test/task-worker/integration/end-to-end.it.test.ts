/**
 * IT-301: End-to-End Integration Test (M1 Simplified)
 *
 * M1 simplified version: verify TaskGenerator → TaskExecutor Complete pipeline
 * - Use 10 test data
 * - No RecordingWorker (manual call)
 * - No TaskScheduler (direct call)
 *
 * Full version will be implemented in M2：
 * - Use RecordingWorker
 * - Use TaskScheduler
 * - Use ActionRecorder
 * - Save actions to database
 */

import { describe, it, expect, beforeAll, afterAll, vi } from 'vitest'
import { TaskGenerator } from '../../../src/task-worker/task-generator'
import { TaskExecutor } from '../../../src/task-worker/task-executor'
import type { TaskExecutorConfig } from '../../../src/task-worker/types'
import {
  getDb,
  sources,
  documents,
  recordingTasks,
  eq,
  sql,
} from '@actionbookdev/db'
import type { Database } from '@actionbookdev/db'

// Mock ActionBuilder to avoid real browser and LLM calls
vi.mock('../../../src/ActionBuilder', () => ({
  ActionBuilder: vi.fn().mockImplementation(() => ({
    initialize: vi.fn().mockResolvedValue(undefined),
    build: vi.fn().mockResolvedValue({
      success: true,
      turns: 5,
      totalDuration: 10000,
      tokens: { input: 1000, output: 500, total: 1500 },
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

describe('IT-301: End-to-End Integration Test (M1)', () => {
  let db: Database
  let generator: TaskGenerator
  let executor: TaskExecutor
  let testSourceId: number
  const testRunId = `it-301-${Date.now()}-${Math.floor(Math.random() * 10000)}`
  const testDomain = `${testRunId}.example.test`

  // Mock config for TaskExecutor
  const mockConfig: TaskExecutorConfig = {
    llmApiKey: 'test-api-key',
    llmBaseURL: 'https://api.test.com/v1',
    llmModel: 'test-model',
    databaseUrl: 'postgres://test:test@localhost:5432/test',
    headless: true,
    maxTurns: 30,
    outputDir: './output',
  }

  beforeAll(async () => {
    // Initialize database
    db = getDb()

    generator = new TaskGenerator(db)
    executor = new TaskExecutor(db, mockConfig)

    // Create test data: 1 source, 10 chunks
    testSourceId = await createTestData(db, testDomain)
  })

  afterAll(async () => {
    // Clean up test data (cascade)
    await db.delete(sources).where(eq(sources.id, testSourceId))
  })

  it('IT-301: TaskGenerator → TaskExecutor Complete pipeline (10 records)', async () => {
    // Step 1: Generate tasks
    const generatedCount = await generator.generate(testSourceId)
    expect(generatedCount).toBe(10) // M1: LIMIT 10

    // Step 2: Verify tasks created
    const tasks = await db
      .select()
      .from(recordingTasks)
      .where(eq(recordingTasks.sourceId, testSourceId))

    expect(tasks).toHaveLength(10)
    expect(tasks.every((t) => t.status === 'pending')).toBe(true)

    // Step 3: Execute all tasks manually (M1: no RecordingWorker)
    for (const task of tasks) {
      const result = await executor.execute(task)
      expect(result.success).toBe(true)
      expect(result.actions_created).toBeGreaterThanOrEqual(0)
    }

    // Step 4: Verify all tasks completed
    const completedTasks = await db
      .select()
      .from(recordingTasks)
      .where(eq(recordingTasks.sourceId, testSourceId))

    expect(completedTasks.every((t) => t.status === 'completed')).toBe(true)
    expect(completedTasks.every((t) => t.progress === 100)).toBe(true)
    expect(completedTasks.every((t) => t.attemptCount === 1)).toBe(true)

    // Step 5: Verify dual-mode prompts were used
    const taskDrivenCount = completedTasks.filter(
      (t) => (t.config as any)?.chunk_type === 'task_driven'
    ).length
    const exploratoryCount = completedTasks.filter(
      (t) => (t.config as any)?.chunk_type === 'exploratory'
    ).length

    expect(taskDrivenCount + exploratoryCount).toBe(10)
    expect(taskDrivenCount).toBeGreaterThan(0) // At least some task-driven
    expect(exploratoryCount).toBeGreaterThan(0) // At least some exploratory

    console.log('✅ IT-301 Complete pipeline test passed:')
    console.log(`   - Generated tasks: ${generatedCount}`)
    console.log(`   - Execution successful: ${completedTasks.length}`)
    console.log(`   - Task-driven: ${taskDrivenCount}`)
    console.log(`   - Exploratory: ${exploratoryCount}`)
  }, 60000) // 60s timeout for integration test
})

/**
 * Create test data: 1 source + 10 chunks (mixed task_driven and exploratory)
 */
async function createTestData(
  db: Database,
  testDomain: string
): Promise<number> {
  // Create source
  const timestamp = Date.now()
  const sourceResult = await db
    .insert(sources)
    .values({
      name: `integration_test_${timestamp}`,
      baseUrl: `https://${testDomain}`,
      description: 'IT-301 test source',
      domain: testDomain,
      crawlConfig: {},
    })
    .returning({ id: sources.id })

  const sourceId = sourceResult[0].id

  // Create 10 chunks (5 task-driven, 5 exploratory)
  for (let i = 0; i < 10; i++) {
    const isTaskDriven = i < 5
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

  return sourceId
}

import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { getDb, buildTasks, eq } from '@actionbookdev/db'

const BASE_URL = process.env.API_URL || 'http://localhost:3100'

/**
 * E2E tests for Build Tasks API
 * Tests: POST, GET list, GET by ID, filtering, error handling
 * Cleanup: All test data is deleted after tests complete
 */
describe('Build Tasks API - E2E Tests', () => {
  // Track created task IDs for cleanup
  const createdTaskIds: number[] = []

  beforeAll(async () => {
    try {
      const res = await fetch(`${BASE_URL}/api/health`)
      if (!res.ok) {
        throw new Error(`Service responded with ${res.status}`)
      }
      console.log('API Service is healthy')
    } catch (error) {
      console.error(`
============================================================
ERROR: Could not connect to API Service at ${BASE_URL}
Please make sure the service is running before running tests.
You can start it with: pnpm dev
============================================================
      `)
      throw error
    }
  })

  afterAll(async () => {
    // Cleanup: Delete all created test tasks
    if (createdTaskIds.length > 0) {
      console.log(`\nCleaning up ${createdTaskIds.length} test tasks...`)
      const db = getDb()

      for (const id of createdTaskIds) {
        try {
          await db.delete(buildTasks).where(eq(buildTasks.id, id))
          console.log(`  Deleted task ${id}`)
        } catch (error) {
          console.error(`  Failed to delete task ${id}:`, error)
        }
      }

      console.log('Cleanup complete.\n')
    }
  })

  // Helper function to create a task and track it for cleanup
  async function createTask(body: Record<string, unknown>): Promise<{
    success: boolean
    data?: Record<string, unknown>
    error?: string
  }> {
    const res = await fetch(`${BASE_URL}/api/build-tasks`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    })

    const data = await res.json()

    if (data.success && data.data?.id) {
      createdTaskIds.push(data.data.id as number)
    }

    return data
  }

  describe('POST /api/build-tasks', () => {
    it('should create a build task with auto-detected "help" category', async () => {
      const result = await createTask({
        sourceUrl: 'https://help.test-example.com/articles',
      })

      expect(result.success).toBe(true)
      expect(result.data).toBeDefined()
      expect(result.data!.id).toBeDefined()
      expect(result.data!.sourceUrl).toBe(
        'https://help.test-example.com/articles'
      )
      expect(result.data!.sourceName).toBe('help.test-example.com')
      expect(result.data!.sourceCategory).toBe('help')
      expect(result.data!.stage).toBe('init')
      expect(result.data!.stageStatus).toBe('pending')
    })

    it('should create a build task with auto-detected "unknown" category', async () => {
      const result = await createTask({
        sourceUrl: 'https://www.test-example.com/pricing',
      })

      expect(result.success).toBe(true)
      expect(result.data!.sourceCategory).toBe('unknown')
      expect(result.data!.sourceName).toBe('www.test-example.com')
    })

    it('should create a build task with explicit category override', async () => {
      const result = await createTask({
        sourceUrl: 'https://www.test-example.com/knowledge-base',
        sourceCategory: 'help',
      })

      expect(result.success).toBe(true)
      expect(result.data!.sourceCategory).toBe('help')
    })

    it('should create a build task with custom sourceName', async () => {
      const result = await createTask({
        sourceUrl: 'https://docs.test-example.com',
        sourceName: 'Test Example Docs',
      })

      expect(result.success).toBe(true)
      expect(result.data!.sourceName).toBe('Test Example Docs')
    })

    it('should create a build task with config', async () => {
      const config = {
        maxDepth: 3,
        includePatterns: ['/help/*', '/docs/*'],
        excludePatterns: ['/admin/*'],
        rateLimit: 1000,
      }

      const result = await createTask({
        sourceUrl: 'https://help.config-test.com',
        config,
      })

      expect(result.success).toBe(true)
      expect(result.data!.config).toBeDefined()
      expect(result.data!.config).toEqual(config)
    })

    it('should create a build task with empty config by default', async () => {
      const result = await createTask({
        sourceUrl: 'https://help.no-config-test.com',
      })

      expect(result.success).toBe(true)
      expect(result.data!.config).toBeDefined()
      expect(result.data!.config).toEqual({})
    })

    it('should return 400 for missing sourceUrl', async () => {
      const res = await fetch(`${BASE_URL}/api/build-tasks`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({}),
      })

      expect(res.status).toBe(400)
      const data = await res.json()
      expect(data.success).toBe(false)
      expect(data.error).toContain('sourceUrl')
    })

    it('should return 400 for invalid URL format', async () => {
      const res = await fetch(`${BASE_URL}/api/build-tasks`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ sourceUrl: 'not-a-valid-url' }),
      })

      expect(res.status).toBe(400)
      const data = await res.json()
      expect(data.success).toBe(false)
      expect(data.error).toContain('Invalid URL')
    })
  })

  describe('GET /api/build-tasks', () => {
    it('should list all build tasks', async () => {
      const res = await fetch(`${BASE_URL}/api/build-tasks`)

      expect(res.status).toBe(200)
      const data = await res.json()

      expect(data.success).toBe(true)
      expect(data.data).toBeDefined()
      expect(Array.isArray(data.data.results)).toBe(true)
      expect(typeof data.data.count).toBe('number')
    })

    it('should filter by category=help', async () => {
      const res = await fetch(`${BASE_URL}/api/build-tasks?category=help`)

      expect(res.status).toBe(200)
      const data = await res.json()

      expect(data.success).toBe(true)
      // All returned tasks should have category=help
      for (const task of data.data.results) {
        expect(task.sourceCategory).toBe('help')
      }
    })

    it('should filter by category=unknown', async () => {
      const res = await fetch(`${BASE_URL}/api/build-tasks?category=unknown`)

      expect(res.status).toBe(200)
      const data = await res.json()

      expect(data.success).toBe(true)
      for (const task of data.data.results) {
        expect(task.sourceCategory).toBe('unknown')
      }
    })

    it('should filter by stage', async () => {
      const res = await fetch(`${BASE_URL}/api/build-tasks?stage=init`)

      expect(res.status).toBe(200)
      const data = await res.json()

      expect(data.success).toBe(true)
      for (const task of data.data.results) {
        expect(task.stage).toBe('init')
      }
    })

    it('should filter by status', async () => {
      const res = await fetch(`${BASE_URL}/api/build-tasks?status=pending`)

      expect(res.status).toBe(200)
      const data = await res.json()

      expect(data.success).toBe(true)
      for (const task of data.data.results) {
        expect(task.stageStatus).toBe('pending')
      }
    })

    it('should respect limit parameter', async () => {
      const res = await fetch(`${BASE_URL}/api/build-tasks?limit=2`)

      expect(res.status).toBe(200)
      const data = await res.json()

      expect(data.success).toBe(true)
      expect(data.data.results.length).toBeLessThanOrEqual(2)
    })

    it('should combine multiple filters', async () => {
      const res = await fetch(
        `${BASE_URL}/api/build-tasks?category=help&stage=init&status=pending&limit=5`
      )

      expect(res.status).toBe(200)
      const data = await res.json()

      expect(data.success).toBe(true)
      for (const task of data.data.results) {
        expect(task.sourceCategory).toBe('help')
        expect(task.stage).toBe('init')
        expect(task.stageStatus).toBe('pending')
      }
    })
  })

  describe('GET /api/build-tasks/:id', () => {
    let testTaskId: number

    beforeAll(async () => {
      // Create a task specifically for this test suite
      const result = await createTask({
        sourceUrl: 'https://help.get-by-id-test.com',
        sourceName: 'Get By ID Test',
      })

      if (result.success && result.data?.id) {
        testTaskId = result.data.id as number
      }
    })

    it('should get task by ID with full details', async () => {
      expect(testTaskId).toBeDefined()

      const res = await fetch(`${BASE_URL}/api/build-tasks/${testTaskId}`)

      expect(res.status).toBe(200)
      const data = await res.json()

      expect(data.success).toBe(true)
      expect(data.data.id).toBe(testTaskId)
      expect(data.data.sourceUrl).toBe('https://help.get-by-id-test.com')
      expect(data.data.sourceName).toBe('Get By ID Test')

      // Verify all expected fields are present
      expect(data.data).toHaveProperty('sourceId')
      expect(data.data).toHaveProperty('sourceCategory')
      expect(data.data).toHaveProperty('stage')
      expect(data.data).toHaveProperty('stageStatus')
      expect(data.data).toHaveProperty('config')
      expect(data.data).toHaveProperty('createdAt')
      expect(data.data).toHaveProperty('updatedAt')
      expect(data.data).toHaveProperty('knowledgeStartedAt')
      expect(data.data).toHaveProperty('knowledgeCompletedAt')
      expect(data.data).toHaveProperty('actionStartedAt')
      expect(data.data).toHaveProperty('actionCompletedAt')
    })

    it('should return 404 for non-existent task ID', async () => {
      const res = await fetch(`${BASE_URL}/api/build-tasks/999999999`)

      expect(res.status).toBe(404)
      const data = await res.json()

      expect(data.success).toBe(false)
      expect(data.error).toContain('not found')
    })

    it('should return 400 for invalid task ID format', async () => {
      const res = await fetch(`${BASE_URL}/api/build-tasks/invalid-id`)

      expect(res.status).toBe(400)
      const data = await res.json()

      expect(data.success).toBe(false)
      expect(data.error).toContain('Invalid')
    })
  })

  describe('URL Category Detection', () => {
    const testCases = [
      { url: 'https://help.example.com', expected: 'help' },
      { url: 'https://support.example.com/tickets', expected: 'help' },
      { url: 'https://docs.example.com/api', expected: 'help' },
      { url: 'https://example.com/faq', expected: 'help' },
      { url: 'https://example.com/documentation/v2', expected: 'help' },
      { url: 'https://www.example.com/pricing', expected: 'unknown' },
      { url: 'https://app.example.com/dashboard', expected: 'unknown' },
      { url: 'https://example.com/about', expected: 'unknown' },
    ]

    for (const { url, expected } of testCases) {
      it(`should detect "${expected}" for ${url}`, async () => {
        const result = await createTask({ sourceUrl: url })

        expect(result.success).toBe(true)
        expect(result.data!.sourceCategory).toBe(expected)
      })
    }
  })

  describe('Data Integrity', () => {
    it('should have correct default values for new task', async () => {
      const result = await createTask({
        sourceUrl: 'https://help.defaults-test.com',
      })

      expect(result.success).toBe(true)
      const task = result.data!

      // Check defaults
      expect(task.sourceId).toBeNull()
      expect(task.stage).toBe('init')
      expect(task.stageStatus).toBe('pending')
      expect(task.config).toEqual({})
      expect(task.knowledgeStartedAt).toBeNull()
      expect(task.knowledgeCompletedAt).toBeNull()
      expect(task.actionStartedAt).toBeNull()
      expect(task.actionCompletedAt).toBeNull()

      // Check timestamps are valid ISO strings
      expect(() => new Date(task.createdAt as string)).not.toThrow()
      expect(() => new Date(task.updatedAt as string)).not.toThrow()
    })

    it('should return tasks ordered by createdAt descending', async () => {
      // Create multiple tasks with small delays
      await createTask({ sourceUrl: 'https://help.order-test-1.com' })
      await new Promise((resolve) => setTimeout(resolve, 50))
      await createTask({ sourceUrl: 'https://help.order-test-2.com' })
      await new Promise((resolve) => setTimeout(resolve, 50))
      await createTask({ sourceUrl: 'https://help.order-test-3.com' })

      const res = await fetch(`${BASE_URL}/api/build-tasks?limit=10`)
      const data = await res.json()

      expect(data.success).toBe(true)

      // Verify descending order
      const results = data.data.results
      for (let i = 0; i < results.length - 1; i++) {
        const current = new Date(results[i].createdAt).getTime()
        const next = new Date(results[i + 1].createdAt).getTime()
        expect(current).toBeGreaterThanOrEqual(next)
      }
    })
  })
})

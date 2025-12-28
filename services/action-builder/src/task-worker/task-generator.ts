/**
 * TaskGenerator - Task Generator
 *
 * Responsible for generating recording_tasks from chunks table
 * M1 version: Only generates LIMIT 10 tasks
 */

import { type Database, sql, type RecordingConfig } from '@actionbookdev/db'
import { detectChunkType } from './utils/chunk-detector.js'
import type { ChunkType } from './types/index.js'

/**
 * Chunk raw data (from database query)
 */
interface ChunkWithMetadata {
  chunkId: number
  sourceId: number
  chunkContent: string
  documentUrl: string
  documentTitle: string
  sourceDomain: string | null
}

export class TaskGenerator {
  constructor(private db: Database) {}

  /**
   * Generate recording tasks
   * M1: Default generates 10 tasks
   *
   * @param sourceId - Optional data source ID filter
   * @param limit - Maximum number of tasks to generate (default 10)
   * @returns Number of tasks generated
   */
  async generate(sourceId?: number, limit: number = 10): Promise<number> {
    // 1. Query chunks with JOIN (M1: default LIMIT 10)
    const chunksData = await this.queryChunks(sourceId, limit)

    // 2. Create recording_task for each chunk
    let createdCount = 0
    for (const chunk of chunksData) {
      try {
        await this.createTask(chunk)
        createdCount++
      } catch (error) {
        // M1: Simplified error handling, log and continue
        console.error(
          `Failed to create task for chunk ${chunk.chunkId}:`,
          error
        )
      }
    }

    return createdCount
  }

  /**
   * Query chunks data (includes document and source info)
   *
   * Only returns chunks that do NOT already have a recording_task.
   * This prevents duplicate task creation on build_task recovery.
   */
  private async queryChunks(
    sourceIdFilter: number | undefined,
    limit: number
  ): Promise<ChunkWithMetadata[]> {
    // Use raw SQL to support NOT EXISTS subquery for deduplication
    const sourceFilter =
      sourceIdFilter !== undefined ? sql`AND s.id = ${sourceIdFilter}` : sql``

    const results = await this.db.execute<{
      chunk_id: number
      source_id: number
      chunk_content: string
      document_url: string
      document_title: string
      source_domain: string | null
    }>(sql`
      SELECT
        c.id AS chunk_id,
        s.id AS source_id,
        c.content AS chunk_content,
        d.url AS document_url,
        d.title AS document_title,
        s.domain AS source_domain
      FROM chunks c
      INNER JOIN documents d ON c.document_id = d.id
      INNER JOIN sources s ON d.source_id = s.id
      WHERE NOT EXISTS (
        SELECT 1 FROM recording_tasks rt
        WHERE rt.chunk_id = c.id
      )
      ${sourceFilter}
      ORDER BY c.id DESC
      LIMIT ${limit}
    `)

    // Debug log
    console.log(
      `[TaskGenerator] queryChunks results: ${results.rows.length} records (excluding existing tasks)`
    )
    for (const r of results.rows) {
      console.log(
        `  - chunkId=${r.chunk_id}, sourceId=${r.source_id}, domain=${r.source_domain}`
      )
    }

    // Map snake_case to camelCase
    return results.rows.map((r) => ({
      chunkId: r.chunk_id,
      sourceId: r.source_id,
      chunkContent: r.chunk_content,
      documentUrl: r.document_url,
      documentTitle: r.document_title,
      sourceDomain: r.source_domain,
    }))
  }

  /**
   * Create recording_task for a single chunk
   */
  private async createTask(chunk: ChunkWithMetadata): Promise<number> {
    // 1. Detect chunk type
    const chunkType: ChunkType = detectChunkType(chunk.chunkContent)

    // 2. Build config
    const config: RecordingConfig = {
      chunk_type: chunkType,
    }

    // Debug: log what we're inserting
    console.log(
      `[TaskGenerator] createTask: chunkId=${chunk.chunkId}, sourceId=${chunk.sourceId}`
    )

    // 3. Use raw SQL insert (bypass Drizzle ORM chunkId issue)
    const configJson = JSON.stringify(config)
    const result = await this.db.execute<{ id: number }>(sql`
      INSERT INTO recording_tasks (source_id, chunk_id, start_url, status, config, attempt_count, scenario)
      VALUES (${chunk.sourceId}, ${chunk.chunkId}, ${chunk.documentUrl}, 'pending', ${configJson}::jsonb, 0, 'default')
      RETURNING id
    `)

    const insertedId = result.rows[0].id
    console.log(`[TaskGenerator] Insert result: id=${insertedId}`)
    return insertedId
  }
}

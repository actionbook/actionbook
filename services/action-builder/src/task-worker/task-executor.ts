/**
 * TaskExecutor - Task Executor (Full Version with ActionBuilder Integration)
 *
 * Uses ActionBuilder to execute recording tasks, supporting dual-mode Prompts:
 * - task_driven: Task-driven mode (uses interact tool)
 * - exploratory: Exploratory mode (uses register_element tool)
 *
 * Flow:
 * 1. Fetch ChunkData from database (JOIN chunks, documents, sources)
 * 2. Use prompt-builder to construct mode-specific Prompt
 * 3. Call ActionBuilder.build() for multi-turn LLM conversation and element recording
 * 4. Update task status (completed/failed)
 * 5. Return ExecutionResult (includes actions_created, turns, tokens, etc.)
 */

import {
  type Database,
  type RecordingTaskStatus,
  recordingTasks,
  chunks,
  documents,
  sources,
  eq,
} from '@actionbookdev/db'
import { ActionBuilder } from '../ActionBuilder.js'
import { DbWriter } from '../writers/DbWriter.js'
import { buildPrompt } from './utils/prompt-builder.js'
import type {
  RecordingTask,
  ExecutionResult,
  TaskExecutorConfig,
  ChunkType,
} from './types/index.js'

/**
 * Extended ChunkData type, includes source information
 */
interface ExtendedChunkData {
  id: number
  document_id: number
  source_id: number
  document_url: string
  document_title: string
  source_domain: string
  source_name: string
  source_base_url: string
  /** App/Product URL for action building (optional, if not set LLM will infer) */
  source_app_url: string | null
  source_description: string
  chunk_content: string
  chunk_index: number
  createdAt: Date
}

export class TaskExecutor {
  private config: TaskExecutorConfig
  private dbWriter: DbWriter
  private buildTimeoutMs: number

  constructor(private db: Database, config: TaskExecutorConfig) {
    this.config = {
      headless: config.headless ?? true,
      maxTurns: config.maxTurns ?? 30,
      outputDir: config.outputDir ?? './output',
      buildTimeoutMinutes: config.buildTimeoutMinutes ?? 8,
      ...config,
    }
    this.buildTimeoutMs = (this.config.buildTimeoutMinutes ?? 8) * 60 * 1000
    this.dbWriter = new DbWriter(db)
  }

  /**
   * Execute recording task
   *
   * @param task - Recording task to execute
   * @returns Execution result
   */
  async execute(task: RecordingTask): Promise<ExecutionResult> {
    const startTime = Date.now()

    // Validate chunkId
    if (task.chunkId === null || task.chunkId === undefined) {
      await this.updateTaskStatus(task.id, {
        status: 'failed',
        errorMessage: 'Chunk ID is required',
        attemptCount: task.attemptCount + 1,
      })
      return {
        success: false,
        actions_created: 0,
        error: 'Chunk ID is required',
        duration_ms: Date.now() - startTime,
      }
    }

    // Fetch ChunkData
    let chunkData: ExtendedChunkData
    try {
      chunkData = await this.fetchChunkData(task.chunkId)
    } catch (error) {
      const errorMessage =
        error instanceof Error ? error.message : String(error)
      await this.updateTaskStatus(task.id, {
        status: 'failed',
        errorMessage,
        attemptCount: task.attemptCount + 1,
      })
      return {
        success: false,
        actions_created: 0,
        error: errorMessage,
        duration_ms: Date.now() - startTime,
      }
    }

    // Log task context information
    console.log(
      `[TaskExecutor] Executing task #${task.id}: ` +
        `source_id=${chunkData.source_id}, ` +
        `document_id=${chunkData.document_id}, ` +
        `chunk_id=${chunkData.id}, ` +
        `name="${chunkData.source_name}", ` +
        `url="${chunkData.document_url}"`
    )

    // Create ActionBuilder instance
    const builder = new ActionBuilder({
      llmApiKey: this.config.llmApiKey,
      llmBaseURL: this.config.llmBaseURL,
      llmModel: this.config.llmModel,
      databaseUrl: this.config.databaseUrl,
      headless: this.config.headless,
      maxTurns: this.config.maxTurns,
      outputDir: this.config.outputDir,
      profileEnabled: this.config.profileEnabled,
      profileDir: this.config.profileDir,
    })

    try {
      // Initialize browser
      await builder.initialize()

      // Build Prompt
      const chunkType = (task.config?.chunk_type as ChunkType) || 'exploratory'
      const { systemPrompt, userPrompt } = this.buildCustomPrompts(
        chunkData,
        chunkType
      )

      // Generate scenario name
      const scenarioName = `task_${task.id}_${Date.now()}`

      // Use root domain of source.base_url as start URL
      // e.g., https://notion.com/help → https://notion.com
      const startUrl = new URL(chunkData.source_base_url).origin

      // Call ActionBuilder.build() with timeout protection
      // Uses configurable timeout (default: 8 minutes)
      const buildPromise = builder.build(startUrl, scenarioName, {
        siteName: chunkData.source_name,
        customSystemPrompt: systemPrompt,
        customUserPrompt: userPrompt,
        taskId: task.id, // Pass existing task ID to avoid duplicate task creation
      })

      const timeoutPromise = new Promise<never>((_, reject) => {
        setTimeout(() => {
          reject(
            new Error(
              `ActionBuilder.build() timeout after ${this.buildTimeoutMs / 1000 / 60} minutes`
            )
          )
        }, this.buildTimeoutMs)
      })

      const buildResult = await Promise.race([buildPromise, timeoutPromise])

      if (buildResult.success) {
        // Success: count elements
        const actionsCreated = this.countElements(buildResult.siteCapability)

        // Update chunks.elements field with discovered elements
        if (task.chunkId && buildResult.siteCapability && actionsCreated > 0) {
          try {
            await this.dbWriter.updateChunkElements(
              task.chunkId,
              buildResult.siteCapability
            )
          } catch (chunkUpdateError) {
            // Log but don't fail the task if chunk update fails
            console.warn(
              `[TaskExecutor] Failed to update chunk elements for chunk ${task.chunkId}:`,
              chunkUpdateError
            )
          }
        }

        // Update task status to completed
        await this.updateTaskStatus(task.id, {
          status: 'completed',
          progress: 100,
          completedAt: new Date(),
          attemptCount: task.attemptCount + 1,
        })

        const duration = Date.now() - startTime
        return {
          success: true,
          actions_created: actionsCreated,
          duration_ms: duration,
          turns: buildResult.turns,
          tokens_used: buildResult.tokens.total,
          saved_path: buildResult.savedPath,
        }
      } else {
        // ActionBuilder returned failure
        const errorMessage = `Recording failed: ${buildResult.message}`
        await this.updateTaskStatus(task.id, {
          status: 'failed',
          errorMessage,
          attemptCount: task.attemptCount + 1,
        })

        const duration = Date.now() - startTime
        return {
          success: false,
          actions_created: 0,
          error: errorMessage,
          duration_ms: duration,
          turns: buildResult.turns,
          tokens_used: buildResult.tokens.total,
        }
      }
    } catch (error) {
      // Execution exception - print full stack trace for debugging
      const errorMessage =
        error instanceof Error ? error.message : String(error)
      const errorStack = error instanceof Error ? error.stack : undefined

      console.error(
        `[TaskExecutor] Task ${task.id} failed with error:`,
        errorMessage
      )
      if (errorStack) {
        console.error(`[TaskExecutor] Stack trace:\n${errorStack}`)
      }

      await this.updateTaskStatus(task.id, {
        status: 'failed',
        errorMessage,
        attemptCount: task.attemptCount + 1,
      })

      const duration = Date.now() - startTime
      return {
        success: false,
        actions_created: 0,
        error: errorMessage,
        duration_ms: duration,
      }
    } finally {
      // Ensure browser is closed
      await builder.close()
    }
  }

  /**
   * Fetch ChunkData from database (JOIN chunks, documents, sources)
   *
   * @param chunkId - Chunk ID
   * @returns ExtendedChunkData object (includes source information)
   * @throws Error if chunk does not exist
   */
  private async fetchChunkData(chunkId: number): Promise<ExtendedChunkData> {
    const result = await this.db
      .select({
        id: chunks.id,
        document_id: documents.id,
        source_id: sources.id,
        document_url: documents.url,
        document_title: documents.title,
        source_domain: sources.domain,
        source_name: sources.name,
        source_base_url: sources.baseUrl,
        source_app_url: sources.appUrl,
        source_description: sources.description,
        chunk_content: chunks.content,
        chunk_index: chunks.chunkIndex,
        createdAt: chunks.createdAt,
      })
      .from(chunks)
      .innerJoin(documents, eq(chunks.documentId, documents.id))
      .innerJoin(sources, eq(documents.sourceId, sources.id))
      .where(eq(chunks.id, chunkId))
      .limit(1)

    if (result.length === 0) {
      throw new Error(`Chunk not found: ${chunkId}`)
    }

    const row = result[0]
    return {
      id: row.id,
      document_id: row.document_id,
      source_id: row.source_id,
      document_url: row.document_url,
      document_title: row.document_title || '',
      source_domain: row.source_domain || '',
      source_name: row.source_name,
      source_base_url: row.source_base_url,
      source_app_url: row.source_app_url,
      source_description: row.source_description || '',
      chunk_content: row.chunk_content,
      chunk_index: row.chunk_index,
      createdAt: row.createdAt,
    }
  }

  /**
   * Build custom Prompt (apply Token limit)
   */
  private buildCustomPrompts(
    chunkData: ExtendedChunkData,
    chunkType: ChunkType
  ): { systemPrompt: string; userPrompt: string } {
    // Convert to ChunkData format
    const chunkForPrompt = {
      id: String(chunkData.id),
      source_id: String(chunkData.source_id),
      document_url: chunkData.document_url,
      document_title: chunkData.document_title,
      source_domain: chunkData.source_domain,
      chunk_content: chunkData.chunk_content,
      chunk_index: chunkData.chunk_index,
      created_at: chunkData.createdAt,
      // Pass app URL for action building (optional, LLM will infer if not set)
      source_app_url: chunkData.source_app_url || undefined,
    }

    // Use prompt-builder (Token limit already handled internally)
    const result = buildPrompt(chunkForPrompt, chunkType)

    return {
      systemPrompt: result.systemPrompt,
      userPrompt: result.userPrompt,
    }
  }

  /**
   * Count elements in siteCapability
   */
  private countElements(siteCapability: any): number {
    if (!siteCapability) return 0

    let count = 0

    // Count elements in pages
    if (siteCapability.pages) {
      for (const pageKey of Object.keys(siteCapability.pages)) {
        const page = siteCapability.pages[pageKey]
        if (page?.elements) {
          count += Object.keys(page.elements).length
        }
      }
    }

    // Count global_elements
    if (siteCapability.global_elements) {
      count += Object.keys(siteCapability.global_elements).length
    }

    return count
  }

  /**
   * Update task status
   */
  private async updateTaskStatus(
    taskId: number,
    updates: {
      status?: RecordingTaskStatus
      progress?: number
      errorMessage?: string
      completedAt?: Date
      attemptCount?: number
    }
  ): Promise<void> {
    await this.db
      .update(recordingTasks)
      .set({
        ...updates,
        updatedAt: new Date(),
      })
      .where(eq(recordingTasks.id, taskId))
  }
}

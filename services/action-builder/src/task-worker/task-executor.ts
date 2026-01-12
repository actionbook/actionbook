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
  and,
  eq,
} from '@actionbookdev/db'
import { ActionBuilder } from '../ActionBuilder.js'
import { DbWriter } from '../writers/DbWriter.js'
import type {
  RecordingTask,
  ExecutionResult,
  TaskExecutorConfig,
} from './types/index.js'
import { buildPrompt } from './utils/prompt-builder.js'

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
      taskTimeoutMinutes: config.taskTimeoutMinutes ?? 10,
      ...config,
    }
    // Use taskTimeoutMinutes for overall timeout (including init, build, cleanup)
    this.buildTimeoutMs = (this.config.taskTimeoutMinutes ?? 10) * 60 * 1000
    this.dbWriter = new DbWriter(db)
  }

  /**
   * Execute recording task
   *
   * @param task - Recording task to execute
   * @returns Execution result
   */
  async execute(task: RecordingTask): Promise<ExecutionResult> {
    const startTime = Date.now();

    // 1. Validate inputs
    if (task.chunkId === null || task.chunkId === undefined) {
      await this.updateTaskStatus(task.id, {
        status: 'failed',
        errorMessage: 'Chunk ID is required',
        attemptCount: task.attemptCount + 1,
      });
      return {
        success: false,
        actions_created: 0,
        error: 'Chunk ID is required',
        duration_ms: 0,
      };
    }

    // 2. Fetch chunk data
    let chunkData: ExtendedChunkData;
    try {
      chunkData = await this.fetchChunkData(task.chunkId);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      await this.updateTaskStatus(task.id, {
        status: 'failed',
        errorMessage,
        attemptCount: task.attemptCount + 1,
      });
      return {
        success: false,
        actions_created: 0,
        error: errorMessage,
        duration_ms: 0,
      };
    }

    console.log(
      `[TaskExecutor] Executing task #${task.id}: ` +
        `source_id=${chunkData.source_id}, ` +
        `document_id=${chunkData.document_id}, ` +
        `chunk_id=${chunkData.id}, ` +
        `name="${chunkData.source_name}", ` +
        `url="${chunkData.document_url}"`
    );

    // 3. Create ActionBuilder (pass timeout config)
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
      buildTimeoutMs: this.buildTimeoutMs, // Pass timeout to builder
    });

    try {
      // 4. Build scenario info
      const scenarioName = `task_${task.id}_${Date.now()}`;
      const startUrl = task.startUrl; // Use task.startUrl (from chunk's document URL)

      // 5. Build dual-mode prompt (task_driven vs exploratory)
      // Pass actionBuilderPrompt from task config for site-specific optimization
      const prompt = buildPrompt(
        {
          id: String(chunkData.id),
          source_id: String(chunkData.source_id),
          document_url: chunkData.document_url,
          document_title: chunkData.document_title,
          source_domain: chunkData.source_domain,
          chunk_content: chunkData.chunk_content,
          chunk_index: chunkData.chunk_index,
          created_at: chunkData.createdAt,
          source_app_url: chunkData.source_app_url ?? undefined,
        },
        task.config?.chunk_type ?? 'exploratory',
        { actionBuilderPrompt: task.config?.actionBuilderPrompt }
      )

      // 6. Call ActionBuilder.build() with custom prompts (task-worker mode)
      const buildResult = await builder.build(startUrl, scenarioName, {
        siteName: chunkData.source_name,
        scenarioDescription: chunkData.chunk_content,
        customSystemPrompt: prompt.systemPrompt,
        customUserPrompt: prompt.userPrompt,
        taskId: task.id,
      });

      // 7. Process result
      if (buildResult.success) {
        const actionsCreated = this.countElements(buildResult.siteCapability);

        // Update chunk elements
        if (task.chunkId && buildResult.siteCapability && actionsCreated > 0) {
          try {
            await this.dbWriter.updateChunkElements(
              task.chunkId,
              buildResult.siteCapability
            );
          } catch (chunkUpdateError) {
            console.warn(
              `[TaskExecutor] Failed to update chunk elements for chunk ${task.chunkId}:`,
              chunkUpdateError
            );
          }
        }

        // Update task status
        // If partialResult (timeout with partial save), record timeout info
        const duration = Date.now() - startTime;
        const statusUpdate: any = {
          status: 'completed',
          progress: 100,
          completedAt: new Date(),
          attemptCount: task.attemptCount + 1,
          durationMs: duration,
          tokensUsed: buildResult.tokens?.total || 0,
        };

        if (buildResult.partialResult) {
          // Keep the original ActionBuilder message for downstream observability
          statusUpdate.errorMessage = buildResult.message;
        }

        await this.updateTaskStatus(task.id, statusUpdate);

        return {
          success: true,
          actions_created: actionsCreated,
          error: buildResult.partialResult ? buildResult.message : undefined,
          duration_ms: duration,
          turns: buildResult.turns || 0,
          tokens_used: buildResult.tokens?.total || 0,
          saved_path: buildResult.savedPath,
        };
      } else {
        // Build failed
        const errorMessage = `Recording failed: ${buildResult.message}`;
        const duration = Date.now() - startTime;
        await this.updateTaskStatus(task.id, {
          status: 'failed',
          errorMessage,
          attemptCount: task.attemptCount + 1,
          completedAt: new Date(),
          durationMs: duration,
          tokensUsed: buildResult.tokens?.total || 0,
        });

        return {
          success: false,
          actions_created: 0,
          error: errorMessage,
          duration_ms: duration,
          turns: buildResult.turns || 0,
          tokens_used: buildResult.tokens?.total || 0,
        };
      }
    } catch (error) {
      // All errors from ActionBuilder.build() are final
      const errorMessage = error instanceof Error ? error.message : String(error);

      const duration = Date.now() - startTime;
      await this.updateTaskStatus(task.id, {
        status: 'failed',
        errorMessage,
        attemptCount: task.attemptCount + 1,
        completedAt: new Date(),
        durationMs: duration,
      });

      return {
        success: false,
        actions_created: 0,
        error: errorMessage,
        duration_ms: duration,
      };
    } finally {
      await builder.close();
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
      durationMs?: number | null
      tokensUsed?: number
    }
  ): Promise<void> {
    // Idempotency/safety: only allow status updates while task is still 'running'.
    // This prevents a "zombie" executor (after stale recovery) from overwriting a re-queued task.
    await this.db
      .update(recordingTasks)
      .set({
        ...updates,
        updatedAt: new Date(),
      })
      .where(and(eq(recordingTasks.id, taskId), eq(recordingTasks.status, 'running')))
  }
}

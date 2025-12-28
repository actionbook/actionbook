/**
 * TaskQuery - M1 Query Tool
 *
 * Provides query interfaces to verify M1 task execution status
 */

import type { Database } from '@actionbookdev/db'
import {
  sources,
  documents,
  chunks,
  recordingTasks,
  eq,
  desc,
} from '@actionbookdev/db'

/**
 * Task Statistics
 */
export interface TaskStats {
  sourceId: number
  sourceDomain: string
  totalTasks: number
  pendingTasks: number
  runningTasks: number
  completedTasks: number
  failedTasks: number
  taskDrivenCount: number
  exploratoryCount: number
}

/**
 * Task Details
 */
export interface TaskDetail {
  taskId: number
  chunkId: number | null
  status: string
  progress: number
  chunkType: string
  attemptCount: number
  errorMessage: string | null
  startUrl: string
  completedAt: Date | null
  createdAt: Date
  updatedAt: Date
  // Joined data
  documentTitle: string | null
  documentUrl: string | null
  chunkContent: string | null
}

export class TaskQuery {
  constructor(private db: Database) {}

  /**
   * Get task statistics for specified source
   */
  async getTaskStats(sourceId: number): Promise<TaskStats | null> {
    // Get source info
    const sourceResult = await this.db
      .select()
      .from(sources)
      .where(eq(sources.id, sourceId))
      .limit(1)

    if (sourceResult.length === 0) {
      return null
    }

    const source = sourceResult[0]

    // Get all tasks
    const tasks = await this.db
      .select()
      .from(recordingTasks)
      .where(eq(recordingTasks.sourceId, sourceId))

    // Calculate stats
    const stats: TaskStats = {
      sourceId,
      sourceDomain: source.domain || 'unknown',
      totalTasks: tasks.length,
      pendingTasks: tasks.filter((t) => t.status === 'pending').length,
      runningTasks: tasks.filter((t) => t.status === 'running').length,
      completedTasks: tasks.filter((t) => t.status === 'completed').length,
      failedTasks: tasks.filter((t) => t.status === 'failed').length,
      taskDrivenCount: tasks.filter(
        (t) => (t.config as any)?.chunk_type === 'task_driven'
      ).length,
      exploratoryCount: tasks.filter(
        (t) => (t.config as any)?.chunk_type === 'exploratory'
      ).length,
    }

    return stats
  }

  /**
   * Get all task details for specified source
   */
  async getTaskDetails(sourceId: number): Promise<TaskDetail[]> {
    const results = await this.db
      .select({
        taskId: recordingTasks.id,
        chunkId: recordingTasks.chunkId,
        status: recordingTasks.status,
        progress: recordingTasks.progress,
        attemptCount: recordingTasks.attemptCount,
        errorMessage: recordingTasks.errorMessage,
        startUrl: recordingTasks.startUrl,
        completedAt: recordingTasks.completedAt,
        createdAt: recordingTasks.createdAt,
        updatedAt: recordingTasks.updatedAt,
        config: recordingTasks.config,
        documentTitle: documents.title,
        documentUrl: documents.url,
        chunkContent: chunks.content,
      })
      .from(recordingTasks)
      .leftJoin(chunks, eq(recordingTasks.chunkId, chunks.id))
      .leftJoin(documents, eq(chunks.documentId, documents.id))
      .where(eq(recordingTasks.sourceId, sourceId))
      .orderBy(desc(recordingTasks.createdAt))

    return results.map((r) => ({
      taskId: r.taskId,
      chunkId: r.chunkId,
      status: r.status,
      progress: r.progress,
      chunkType: (r.config as any)?.chunk_type || 'unknown',
      attemptCount: r.attemptCount,
      errorMessage: r.errorMessage,
      startUrl: r.startUrl,
      completedAt: r.completedAt,
      createdAt: r.createdAt,
      updatedAt: r.updatedAt,
      documentTitle: r.documentTitle,
      documentUrl: r.documentUrl,
      chunkContent: r.chunkContent,
    }))
  }

  /**
   * Search tasks (by keyword)
   */
  async searchTasks(keyword: string): Promise<TaskDetail[]> {
    // Search in document titles and URLs
    const results = await this.db
      .select({
        taskId: recordingTasks.id,
        chunkId: recordingTasks.chunkId,
        status: recordingTasks.status,
        progress: recordingTasks.progress,
        attemptCount: recordingTasks.attemptCount,
        errorMessage: recordingTasks.errorMessage,
        startUrl: recordingTasks.startUrl,
        completedAt: recordingTasks.completedAt,
        createdAt: recordingTasks.createdAt,
        updatedAt: recordingTasks.updatedAt,
        config: recordingTasks.config,
        documentTitle: documents.title,
        documentUrl: documents.url,
        chunkContent: chunks.content,
      })
      .from(recordingTasks)
      .leftJoin(chunks, eq(recordingTasks.chunkId, chunks.id))
      .leftJoin(documents, eq(chunks.documentId, documents.id))
      .orderBy(desc(recordingTasks.createdAt))
      .limit(50) // Limit to 50 results

    // Filter in memory (simple keyword match)
    const filtered = results.filter((r) => {
      const searchText = [r.documentTitle, r.documentUrl, r.startUrl]
        .filter(Boolean)
        .join(' ')
        .toLowerCase()

      return searchText.includes(keyword.toLowerCase())
    })

    return filtered.map((r) => ({
      taskId: r.taskId,
      chunkId: r.chunkId,
      status: r.status,
      progress: r.progress,
      chunkType: (r.config as any)?.chunk_type || 'unknown',
      attemptCount: r.attemptCount,
      errorMessage: r.errorMessage,
      startUrl: r.startUrl,
      completedAt: r.completedAt,
      createdAt: r.createdAt,
      updatedAt: r.updatedAt,
      documentTitle: r.documentTitle,
      documentUrl: r.documentUrl,
      chunkContent: r.chunkContent,
    }))
  }

  /**
   * Get statistics for all sources
   */
  async getAllSourcesStats(): Promise<TaskStats[]> {
    const allSources = await this.db.select().from(sources)

    const stats: TaskStats[] = []
    for (const source of allSources) {
      const stat = await this.getTaskStats(source.id)
      if (stat) {
        stats.push(stat)
      }
    }

    return stats.filter((s) => s.totalTasks > 0) // Only return sources with tasks
  }
}

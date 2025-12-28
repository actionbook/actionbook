/**
 * Storage - PostgreSQL data persistence
 *
 * Handles all database operations for the knowledge builder using Drizzle ORM.
 */

import {
  getDb,
  sources,
  sourceVersions,
  documents,
  chunks,
  crawlLogs,
  eq,
  sql,
  desc,
  and,
} from '@actionbookdev/db'
import type {
  CrawlStatus,
  Database,
  DocumentStatus,
  SourceVersionStatus,
} from '@actionbookdev/db'
import type {
  Source,
  SourceVersion,
  CrawlLog,
  DocumentResult,
  CreateSourceInput,
  CreateVersionInput,
  UpsertDocumentInput,
  CreateChunkInput,
  UpdateCrawlLogInput,
} from './types.js'

// Transaction client type
type TransactionClient = Parameters<Parameters<Database['transaction']>[0]>[0]

/**
 * Storage class for PostgreSQL operations
 */
export class Storage {
  private db: ReturnType<typeof getDb> | TransactionClient
  private isTransaction: boolean

  constructor(db?: TransactionClient) {
    this.db = db || getDb()
    this.isTransaction = !!db
  }

  // ============================================
  // Source operations
  // ============================================

  async createSource(input: CreateSourceInput): Promise<Source> {
    const result = await this.db
      .insert(sources)
      .values({
        name: input.name,
        baseUrl: input.baseUrl,
        description: input.description,
        crawlConfig: input.crawlConfig,
      })
      .returning()

    return this.mapSource(result[0])
  }

  async getSourceByName(name: string): Promise<Source | null> {
    const result = await this.db
      .select()
      .from(sources)
      .where(eq(sources.name, name))
      .limit(1)

    return result[0] ? this.mapSource(result[0]) : null
  }

  async getSourceById(id: number): Promise<Source | null> {
    const result = await this.db
      .select()
      .from(sources)
      .where(eq(sources.id, id))
      .limit(1)

    return result[0] ? this.mapSource(result[0]) : null
  }

  async updateSourceLastCrawled(id: number): Promise<void> {
    await this.db
      .update(sources)
      .set({ lastCrawledAt: new Date() })
      .where(eq(sources.id, id))
  }

  // ============================================
  // Version operations (Blue/Green deployment)
  // ============================================

  async createVersion(input: CreateVersionInput): Promise<SourceVersion> {
    // Get next version number
    const latestVersion = await this.db
      .select({ versionNumber: sourceVersions.versionNumber })
      .from(sourceVersions)
      .where(eq(sourceVersions.sourceId, input.sourceId))
      .orderBy(desc(sourceVersions.versionNumber))
      .limit(1)

    const nextVersionNumber = (latestVersion[0]?.versionNumber ?? 0) + 1

    const result = await this.db
      .insert(sourceVersions)
      .values({
        sourceId: input.sourceId,
        versionNumber: nextVersionNumber,
        status: 'building' as SourceVersionStatus,
        commitMessage: input.commitMessage,
        createdBy: input.createdBy,
      })
      .returning()

    return this.mapSourceVersion(result[0])
  }

  async getVersionById(id: number): Promise<SourceVersion | null> {
    const result = await this.db
      .select()
      .from(sourceVersions)
      .where(eq(sourceVersions.id, id))
      .limit(1)

    return result[0] ? this.mapSourceVersion(result[0]) : null
  }

  async publishVersion(versionId: number): Promise<void> {
    const version = await this.getVersionById(versionId)
    if (!version) {
      throw new Error(`Version ${versionId} not found`)
    }

    const publishedAt = new Date()

    // Get current active version
    const source = await this.db
      .select({ currentVersionId: sources.currentVersionId })
      .from(sources)
      .where(eq(sources.id, version.sourceId))
      .limit(1)

    const oldActiveVersionId = source[0]?.currentVersionId

    // Perform atomic switch
    // 1. Archive old active version (if exists)
    if (oldActiveVersionId) {
      await this.db
        .update(sourceVersions)
        .set({ status: 'archived' as SourceVersionStatus })
        .where(eq(sourceVersions.id, oldActiveVersionId))
    }

    // 2. Set new version to active
    await this.db
      .update(sourceVersions)
      .set({
        status: 'active' as SourceVersionStatus,
        publishedAt,
      })
      .where(eq(sourceVersions.id, versionId))

    // 3. Update source's currentVersionId
    await this.db
      .update(sources)
      .set({
        currentVersionId: versionId,
        updatedAt: publishedAt,
      })
      .where(eq(sources.id, version.sourceId))
  }

  async deleteVersion(versionId: number): Promise<void> {
    // Cascade will delete documents and chunks
    await this.db.delete(sourceVersions).where(eq(sourceVersions.id, versionId))
  }

  // ============================================
  // Document operations
  // ============================================

  async upsertDocument(input: UpsertDocumentInput): Promise<DocumentResult> {
    // When sourceVersionId is provided, use version-scoped upsert
    if (input.sourceVersionId) {
      // Check if document exists in this version
      const existing = await this.db
        .select({
          id: documents.id,
          contentHash: documents.contentHash,
          version: documents.version,
        })
        .from(documents)
        .where(
          and(
            eq(documents.sourceVersionId, input.sourceVersionId),
            eq(documents.urlHash, input.urlHash)
          )
        )
        .limit(1)

      if (existing.length > 0) {
        // Update existing document in this version
        const result = await this.db
          .update(documents)
          .set({
            title: input.title,
            description: input.description,
            contentText: input.contentText,
            contentHtml: input.contentHtml,
            contentMd: input.contentMd,
            breadcrumb: input.breadcrumb,
            wordCount: input.wordCount,
            contentHash: input.contentHash,
            version: existing[0].version + 1,
            updatedAt: new Date(),
          })
          .where(eq(documents.id, existing[0].id))
          .returning({ id: documents.id })

        return { id: result[0].id }
      } else {
        // Insert new document with version
        const result = await this.db
          .insert(documents)
          .values({
            sourceId: input.sourceId,
            sourceVersionId: input.sourceVersionId,
            url: input.url,
            urlHash: input.urlHash,
            title: input.title,
            description: input.description,
            contentText: input.contentText,
            contentHtml: input.contentHtml,
            contentMd: input.contentMd,
            parentId: input.parentId,
            depth: input.depth,
            breadcrumb: input.breadcrumb,
            wordCount: input.wordCount,
            language: input.language,
            contentHash: input.contentHash,
            status: input.status as DocumentStatus,
            version: input.version,
          })
          .returning({ id: documents.id })

        return { id: result[0].id }
      }
    }

    // Legacy mode: source-scoped upsert (for backward compatibility)
    const existing = await this.db
      .select({
        id: documents.id,
        contentHash: documents.contentHash,
        version: documents.version,
      })
      .from(documents)
      .where(
        sql`${documents.sourceId} = ${input.sourceId} AND ${documents.urlHash} = ${input.urlHash}`
      )
      .limit(1)

    if (existing.length > 0) {
      // Update existing document
      const result = await this.db
        .update(documents)
        .set({
          title: input.title,
          description: input.description,
          contentText: input.contentText,
          contentHtml: input.contentHtml,
          contentMd: input.contentMd,
          breadcrumb: input.breadcrumb,
          wordCount: input.wordCount,
          contentHash: input.contentHash,
          version: existing[0].version + 1,
          updatedAt: new Date(),
        })
        .where(eq(documents.id, existing[0].id))
        .returning({ id: documents.id })

      return { id: result[0].id }
    } else {
      // Insert new document
      const result = await this.db
        .insert(documents)
        .values({
          sourceId: input.sourceId,
          url: input.url,
          urlHash: input.urlHash,
          title: input.title,
          description: input.description,
          contentText: input.contentText,
          contentHtml: input.contentHtml,
          contentMd: input.contentMd,
          parentId: input.parentId,
          depth: input.depth,
          breadcrumb: input.breadcrumb,
          wordCount: input.wordCount,
          language: input.language,
          contentHash: input.contentHash,
          status: input.status as DocumentStatus,
          version: input.version,
        })
        .returning({ id: documents.id })

      return { id: result[0].id }
    }
  }

  async getDocumentContentHash(
    sourceId: number,
    urlHash: string
  ): Promise<string | null> {
    const result = await this.db
      .select({ contentHash: documents.contentHash })
      .from(documents)
      .where(
        sql`${documents.sourceId} = ${sourceId} AND ${documents.urlHash} = ${urlHash}`
      )
      .limit(1)

    return result[0]?.contentHash ?? null
  }

  async getDocumentContentHashByVersion(
    versionId: number,
    urlHash: string
  ): Promise<string | null> {
    const result = await this.db
      .select({ contentHash: documents.contentHash })
      .from(documents)
      .where(
        and(
          eq(documents.sourceVersionId, versionId),
          eq(documents.urlHash, urlHash)
        )
      )
      .limit(1)

    return result[0]?.contentHash ?? null
  }

  // ============================================
  // Chunk operations
  // ============================================

  async deleteChunksByDocument(documentId: number): Promise<void> {
    await this.db.delete(chunks).where(eq(chunks.documentId, documentId))
  }

  async insertChunks(chunkList: CreateChunkInput[]): Promise<void> {
    // Filter out chunks with empty content
    const validChunks = chunkList.filter(
      (chunk) => chunk.content && chunk.content.trim().length > 0
    )
    if (validChunks.length === 0) return

    // Insert chunks one by one, checking for duplicates before each insert
    for (const chunk of validChunks) {
      // Check if chunk already exists (by content_hash + source_version_id)
      if (chunk.sourceVersionId != null) {
        const existing = await this.db
          .select({ id: chunks.id })
          .from(chunks)
          .where(
            and(
              eq(chunks.contentHash, chunk.contentHash),
              eq(chunks.sourceVersionId, chunk.sourceVersionId)
            )
          )
          .limit(1)

        if (existing.length > 0) {
          continue // Skip duplicate
        }
      }

      if (chunk.embedding) {
        // Use raw SQL for vector embedding
        const headingJson = JSON.stringify(chunk.headingHierarchy)
        const embeddingStr = `[${chunk.embedding.join(',')}]`
        await this.db.execute(sql`
          INSERT INTO chunks (
            document_id, source_version_id, content, content_hash, chunk_index,
            start_char, end_char, heading, heading_hierarchy,
            token_count, embedding, embedding_model
          ) VALUES (
            ${chunk.documentId},
            ${chunk.sourceVersionId ?? null},
            ${chunk.content},
            ${chunk.contentHash},
            ${chunk.chunkIndex},
            ${chunk.startChar},
            ${chunk.endChar},
            ${chunk.heading},
            ${headingJson}::jsonb,
            ${chunk.tokenCount},
            ${embeddingStr}::vector,
            ${chunk.embeddingModel}
          )
        `)
      } else {
        await this.db.insert(chunks).values({
          documentId: chunk.documentId,
          sourceVersionId: chunk.sourceVersionId,
          content: chunk.content,
          contentHash: chunk.contentHash,
          chunkIndex: chunk.chunkIndex,
          startChar: chunk.startChar,
          endChar: chunk.endChar,
          heading: chunk.heading,
          headingHierarchy: chunk.headingHierarchy,
          tokenCount: chunk.tokenCount,
          embeddingModel: chunk.embeddingModel,
        })
      }
    }
  }

  // ============================================
  // Crawl log operations
  // ============================================

  async createCrawlLog(sourceId: number): Promise<CrawlLog> {
    const result = await this.db
      .insert(crawlLogs)
      .values({
        sourceId,
        status: 'running' as CrawlStatus,
      })
      .returning()

    return this.mapCrawlLog(result[0])
  }

  async updateCrawlLog(id: number, input: UpdateCrawlLogInput): Promise<void> {
    const updates: Record<string, unknown> = {}

    if (input.status !== undefined) {
      updates.status = input.status
      if (input.status === 'completed' || input.status === 'failed') {
        updates.finishedAt = new Date()
      }
    }
    if (input.pagesCrawled !== undefined)
      updates.pagesCrawled = input.pagesCrawled
    if (input.pagesUpdated !== undefined)
      updates.pagesUpdated = input.pagesUpdated
    if (input.pagesNew !== undefined) updates.pagesNew = input.pagesNew
    if (input.errorCount !== undefined) updates.errorCount = input.errorCount
    if (input.errorDetails !== undefined)
      updates.errorDetails = input.errorDetails

    await this.db.update(crawlLogs).set(updates).where(eq(crawlLogs.id, id))
  }

  // ============================================
  // Transaction support
  // ============================================

  async withTransaction<T>(fn: (storage: Storage) => Promise<T>): Promise<T> {
    if (this.isTransaction) {
      // Already in a transaction, just execute
      return fn(this)
    }

    const db = getDb()
    return db.transaction(async (tx) => {
      const txStorage = new Storage(tx)
      return fn(txStorage)
    })
  }

  // ============================================
  // Mapping helpers
  // ============================================

  private mapSource(row: typeof sources.$inferSelect): Source {
    return {
      id: row.id,
      name: row.name,
      baseUrl: row.baseUrl,
      description: row.description,
      crawlConfig: row.crawlConfig,
      lastCrawledAt: row.lastCrawledAt,
      createdAt: row.createdAt,
      updatedAt: row.updatedAt,
      currentVersionId: row.currentVersionId,
    }
  }

  private mapSourceVersion(
    row: typeof sourceVersions.$inferSelect
  ): SourceVersion {
    return {
      id: row.id,
      sourceId: row.sourceId,
      versionNumber: row.versionNumber,
      status: row.status,
      commitMessage: row.commitMessage,
      createdBy: row.createdBy,
      createdAt: row.createdAt,
      publishedAt: row.publishedAt,
    }
  }

  private mapCrawlLog(row: typeof crawlLogs.$inferSelect): CrawlLog {
    return {
      id: row.id,
      sourceId: row.sourceId,
      status: row.status,
      startedAt: row.startedAt,
      finishedAt: row.finishedAt,
      pagesCrawled: row.pagesCrawled,
      pagesNew: row.pagesNew,
      pagesUpdated: row.pagesUpdated,
      errorCount: row.errorCount,
      errorDetails: row.errorDetails as Record<string, unknown> | null,
    }
  }
}

/**
 * Create a Storage instance
 */
export function createStorage(): Storage {
  return new Storage()
}

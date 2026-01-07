/**
 * Storage - PostgreSQL data persistence
 *
 * Handles all database operations for the playbook builder using Drizzle ORM.
 */

import {
  getDb,
  sources,
  sourceVersions,
  documents,
  chunks,
  eq,
  and,
  desc,
  sql,
} from '@actionbookdev/db';
import type { Database, SourceVersionStatus } from '@actionbookdev/db';
import { createHash } from 'crypto';
import { log } from '../utils/index.js';
import type {
  CreatePlaybookInput,
  CreateVersionInput,
  PlaybookResult,
  VersionResult,
} from './types.js';

// Transaction client type
type TransactionClient = Parameters<Parameters<Database['transaction']>[0]>[0];

/**
 * Storage class for PostgreSQL operations
 */
export class Storage {
  private db: ReturnType<typeof getDb> | TransactionClient;
  private isTransaction: boolean;

  constructor(db?: TransactionClient) {
    this.db = db || getDb();
    this.isTransaction = !!db;
  }

  // ============================================
  // Source Version operations
  // ============================================

  async createVersion(input: CreateVersionInput): Promise<VersionResult> {
    const latestVersion = await this.db
      .select({ versionNumber: sourceVersions.versionNumber })
      .from(sourceVersions)
      .where(eq(sourceVersions.sourceId, input.sourceId))
      .orderBy(desc(sourceVersions.versionNumber))
      .limit(1);

    const nextVersionNumber = (latestVersion[0]?.versionNumber ?? 0) + 1;

    const [result] = await this.db
      .insert(sourceVersions)
      .values({
        sourceId: input.sourceId,
        versionNumber: nextVersionNumber,
        status: 'building' as SourceVersionStatus,
        commitMessage: input.commitMessage || `Playbook build - ${new Date().toISOString()}`,
        createdBy: input.createdBy || 'playbook-builder',
      })
      .returning({ id: sourceVersions.id });

    log('info', `[Storage] Created source version ${nextVersionNumber} (id: ${result.id})`);
    return { id: result.id, versionNumber: nextVersionNumber };
  }

  async publishVersion(versionId: number, sourceId: number): Promise<void> {
    // Archive old active versions
    await this.db
      .update(sourceVersions)
      .set({ status: 'archived' as SourceVersionStatus })
      .where(
        and(
          eq(sourceVersions.sourceId, sourceId),
          eq(sourceVersions.status, 'active')
        )
      );

    // Set new version as active
    await this.db
      .update(sourceVersions)
      .set({
        status: 'active' as SourceVersionStatus,
        publishedAt: new Date(),
      })
      .where(eq(sourceVersions.id, versionId));

    // Update source's currentVersionId
    await this.db
      .update(sources)
      .set({
        currentVersionId: versionId,
        updatedAt: new Date(),
      })
      .where(eq(sources.id, sourceId));

    log('info', `[Storage] Published source version ${versionId}`);
  }

  // ============================================
  // Playbook (Document + Chunk) operations
  // ============================================

  /**
   * Create a playbook document with its associated chunk
   * One document = one chunk (page capabilities description)
   */
  async createPlaybook(input: CreatePlaybookInput): Promise<PlaybookResult> {
    const urlHash = createHash('sha256').update(input.url).digest('hex').slice(0, 64);
    const contentHash = createHash('sha256').update(input.chunkContent).digest('hex').slice(0, 64);

    // Create document
    const [doc] = await this.db
      .insert(documents)
      .values({
        sourceId: input.sourceId,
        sourceVersionId: input.sourceVersionId,
        url: input.url,
        urlHash,
        title: input.title,
        description: input.description,
        status: 'active',
      })
      .returning({ id: documents.id });

    // Create chunk with embedding
    let chunkId: number;
    if (input.embedding) {
      const embeddingStr = `[${input.embedding.join(',')}]`;
      const result = await this.db.execute(sql`
        INSERT INTO chunks (
          document_id, source_version_id, content, content_hash, chunk_index,
          start_char, end_char, heading, token_count, embedding, embedding_model
        ) VALUES (
          ${doc.id},
          ${input.sourceVersionId},
          ${input.chunkContent},
          ${contentHash},
          0,
          0,
          ${input.chunkContent.length},
          ${input.title},
          ${Math.ceil(input.chunkContent.length / 4)},
          ${embeddingStr}::vector,
          ${input.embeddingModel || null}
        )
        RETURNING id
      `);
      const rows = result.rows as { id: number }[];
      chunkId = rows[0].id;
    } else {
      const [chunk] = await this.db.insert(chunks).values({
        documentId: doc.id,
        sourceVersionId: input.sourceVersionId,
        content: input.chunkContent,
        contentHash,
        chunkIndex: 0,
        startChar: 0,
        endChar: input.chunkContent.length,
        heading: input.title,
        tokenCount: Math.ceil(input.chunkContent.length / 4),
      }).returning({ id: chunks.id });
      chunkId = chunk.id;
    }

    log('info', `[Storage] Created playbook: doc=${doc.id}, chunk=${chunkId}, title="${input.title}"`);
    return { documentId: doc.id, chunkId };
  }

  // ============================================
  // Transaction support
  // ============================================

  async withTransaction<T>(fn: (storage: Storage) => Promise<T>): Promise<T> {
    if (this.isTransaction) {
      return fn(this);
    }

    const db = getDb();
    return db.transaction(async (tx) => {
      const txStorage = new Storage(tx);
      return fn(txStorage);
    });
  }
}

/**
 * Create a Storage instance
 */
export function createStorage(): Storage {
  return new Storage();
}

import {
  pgTable,
  serial,
  integer,
  text,
  varchar,
  jsonb,
  timestamp,
  vector,
} from 'drizzle-orm/pg-core';
import { documents } from './document';

/**
 * Chunks table - Document chunks table
 * Stores document chunks, vector embeddings, and playbook actions
 */
export const chunks = pgTable('chunks', {
  id: serial('id').primaryKey(),
  documentId: integer('document_id')
    .notNull()
    .references(() => documents.id, { onDelete: 'cascade' }),
  /** Source version ID (redundant, no FK constraint) */
  sourceVersionId: integer('source_version_id'),
  content: text('content').notNull(),
  contentHash: varchar('content_hash', { length: 64 }).notNull(),
  chunkIndex: integer('chunk_index').notNull(),
  startChar: integer('start_char').notNull(),
  endChar: integer('end_char').notNull(),
  heading: text('heading'),
  headingHierarchy: jsonb('heading_hierarchy').$type<HeadingItem[]>().default([]),
  tokenCount: integer('token_count').notNull(),
  embedding: vector('embedding', { dimensions: 1536 }),
  embeddingModel: varchar('embedding_model', { length: 50 }),
  /** @deprecated Kept for backward compatibility. */
  elements: text('elements'),

  createdAt: timestamp('created_at', { withTimezone: true }).notNull().defaultNow(),
});

/**
 * HeadingItem - Heading hierarchy item
 */
export interface HeadingItem {
  level: number;
  text: string;
}

/**
 * ActionCategory - Category of an action
 */
export type ActionCategory = 'navigation' | 'form' | 'data' | 'other';

/**
 * ActionStatus - Status of an action
 */
export type ActionStatus = 'discovered' | 'valid' | 'invalid';


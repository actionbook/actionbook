import {
  pgTable,
  serial,
  integer,
  text,
  varchar,
  jsonb,
  timestamp,
  unique,
  index,
} from 'drizzle-orm/pg-core';
import { sources } from './source';
import { sourceVersions } from './source-version';

/**
 * Documents table - Documents table
 * Stores crawled web documents and playbooks
 */
export const documents = pgTable(
  'documents',
  {
    id: serial('id').primaryKey(),
    sourceId: integer('source_id')
      .notNull()
      .references(() => sources.id, { onDelete: 'cascade' }),

    /** Associated version ID (Blue/Green deployment) - optional, for backward compatibility */
    sourceVersionId: integer('source_version_id')
      .references(() => sourceVersions.id, { onDelete: 'cascade' }),

    url: text('url').notNull(),
    urlHash: varchar('url_hash', { length: 64 }).notNull(),
    title: text('title'),
    description: text('description'),
    contentText: text('content_text'),
    contentHtml: text('content_html'),
    contentMd: text('content_md'),
    parentId: integer('parent_id'),
    depth: integer('depth').notNull().default(0),
    breadcrumb: jsonb('breadcrumb').$type<BreadcrumbItem[]>().default([]),
    wordCount: integer('word_count'),
    language: varchar('language', { length: 10 }).default('en'),
    contentHash: varchar('content_hash', { length: 64 }),
    /** @deprecated Kept for backward compatibility */
    elements: text('elements'),

    status: varchar('status', { length: 20 }).$type<DocumentStatus>().notNull().default('active'),
    version: integer('version').notNull().default(1),
    publishedAt: timestamp('published_at', { withTimezone: true }),
    crawledAt: timestamp('crawled_at', { withTimezone: true }).notNull().defaultNow(),
    updatedAt: timestamp('updated_at', { withTimezone: true }).notNull().defaultNow(),
    // Note: content_tsv is a generated column, handled by PostgreSQL
  },
  (table) => [
    // Version-scoped urlHash unique index (Blue/Green deployment)
    // Note: Removed (source_id, url_hash) unique index because different versions of the same source can have the same url_hash
    unique('documents_version_url_unique').on(table.sourceVersionId, table.urlHash),
    // Version query index
    index('documents_version_id_idx').on(table.sourceVersionId),
    // source_id index (for querying all documents of a source)
    index('documents_source_id_idx').on(table.sourceId),
  ]
);

/**
 * DocumentStatus - Document status
 */
export type DocumentStatus = 'active' | 'archived' | 'deleted' | 'pending';

/**
 * BreadcrumbItem - Breadcrumb item
 */
export interface BreadcrumbItem {
  title: string;
  url: string;
}


import {
  pgTable,
  serial,
  integer,
  varchar,
  text,
  timestamp,
  index,
  jsonb,
} from 'drizzle-orm/pg-core';
import { sources } from './source';
import { sourceVersions } from './source-version';

/**
 * BuildTasks table - Build pipeline tasks
 * Tracks the overall build process including knowledge-builder and action-builder stages
 */
export const buildTasks = pgTable(
  'build_tasks',
  {
    id: serial('id').primaryKey(),

    // ========== Source association ==========
    /** Associated source ID (created after knowledge build starts) */
    sourceId: integer('source_id').references(() => sources.id, {
      onDelete: 'set null',
    }),
    /** Associated source version ID (1:1 relationship with build_task) */
    sourceVersionId: integer('source_version_id').references(() => sourceVersions.id, {
      onDelete: 'set null',
    }),
    /** Start URL */
    sourceUrl: text('source_url').notNull(),
    /** Site name */
    sourceName: text('source_name'),
    /** Source category: help center or unknown */
    sourceCategory: varchar('source_category', { length: 20 })
      .$type<SourceCategory>()
      .notNull()
      .default('unknown'),

    // ========== Stage management ==========
    /** Current stage */
    stage: varchar('stage', { length: 20 })
      .$type<BuildTaskStage>()
      .notNull()
      .default('init'),
    /** Stage status */
    stageStatus: varchar('stage_status', { length: 20 })
      .$type<BuildTaskStageStatus>()
      .notNull()
      .default('pending'),

    // ========== Configuration ==========
    /** Additional configuration (crawl depth, patterns, etc.) - internal use */
    config: jsonb('config').$type<BuildTaskConfig>().default({}),

    // ========== Error tracking ==========
    /** Error message when task fails */
    errorMessage: text('error_message'),

    // ========== Timestamps ==========
    /** Created time */
    createdAt: timestamp('created_at', { withTimezone: true })
      .notNull()
      .defaultNow(),
    /** Updated time */
    updatedAt: timestamp('updated_at', { withTimezone: true })
      .notNull()
      .defaultNow(),
    /** Knowledge stage start time */
    knowledgeStartedAt: timestamp('knowledge_started_at', {
      withTimezone: true,
    }),
    /** Knowledge stage completion time */
    knowledgeCompletedAt: timestamp('knowledge_completed_at', {
      withTimezone: true,
    }),
    /** Action stage start time */
    actionStartedAt: timestamp('action_started_at', { withTimezone: true }),
    /** Action stage completion time */
    actionCompletedAt: timestamp('action_completed_at', { withTimezone: true }),
  },
  (table) => ({
    stageStatusIdx: index('idx_build_tasks_stage_status').on(
      table.stage,
      table.stageStatus
    ),
    sourceCategoryIdx: index('idx_build_tasks_source_category').on(
      table.sourceCategory
    ),
    sourceIdIdx: index('idx_build_tasks_source_id').on(table.sourceId),
  })
);

// ============================================================================
// JSON Types
// ============================================================================

/**
 * SourceCategory - Source category type
 * - 'help': Help center / documentation site
 * - 'unknown': Unknown category (legacy)
 * - 'any': General website (processed by knowledge-builder-any)
 * - 'playbook': Playbook crawl data (crawl-playbook output)
 */
export type SourceCategory = 'help' | 'unknown' | 'any' | 'playbook';

/**
 * BuildTaskStage - Build task stage
 */
export type BuildTaskStage =
  | 'init' // Initial state, waiting for knowledge build
  | 'knowledge_build' // Knowledge/Playbook building in progress
  | 'action_build' // Action building in progress
  | 'completed' // All stages completed
  | 'error'; // Permanent error (max retries exceeded)

/**
 * BuildTaskStageStatus - Stage execution status
 */
export type BuildTaskStageStatus =
  | 'pending' // Waiting to execute
  | 'running' // Currently executing
  | 'completed' // Stage completed successfully
  | 'error'; // Stage failed

/**
 * BuildTaskConfig - Build task configuration (internal use)
 */
export interface BuildTaskConfig {
  /** Crawl depth */
  maxDepth?: number;
  /** Include URL patterns */
  includePatterns?: string[];
  /** Exclude URL patterns */
  excludePatterns?: string[];
  /** Rate limit in milliseconds */
  rateLimit?: number;

  // ========== Playbook Builder Config ==========
  /** Maximum pages to process in playbook builder */
  playbookMaxPages?: number;
  /** Maximum depth for recursive page discovery in playbook builder (default: 1) */
  playbookMaxDepth?: number;
  /** Run playbook builder in headless mode */
  playbookHeadless?: boolean;

  /** Additional configuration */
  [key: string]: unknown;
}
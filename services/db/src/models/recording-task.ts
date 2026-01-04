import {
  pgTable,
  serial,
  integer,
  varchar,
  text,
  jsonb,
  timestamp,
  unique,
} from 'drizzle-orm/pg-core';
import { sources } from './source';
import { chunks } from './chunk';
import { buildTasks } from './build-task';

/**
 * RecordingTasks table - Recording tasks table
 * Records the execution status of each recording task
 */
export const recordingTasks = pgTable(
  'recording_tasks',
  {
    id: serial('id').primaryKey(),
    /** Parent site */
    sourceId: integer('source_id')
      .notNull()
      .references(() => sources.id, { onDelete: 'cascade' }),
    /** Associated build task ID (links to build pipeline) */
    buildTaskId: integer('build_task_id')
      .references(() => buildTasks.id, { onDelete: 'cascade' }),
    /** Associated chunk ID (Task Worker mode) */
    chunkId: integer('chunk_id')
      .references(() => chunks.id, { onDelete: 'set null' }),
    /** Scenario name (optional, determined by chunk in Task Worker mode) */
    scenario: varchar('scenario', { length: 255 }),
    /** Start URL */
    startUrl: text('start_url').notNull(),
    /** Status */
    status: varchar('status', { length: 20 }).$type<RecordingTaskStatus>().notNull().default('pending'),
    /** Progress (0-100) */
    progress: integer('progress').notNull().default(0),
    /** Number of elements discovered */
    elementsDiscovered: integer('elements_discovered').notNull().default(0),
    /** Number of pages discovered */
    pagesDiscovered: integer('pages_discovered').notNull().default(0),
    /** Tokens consumed */
    tokensUsed: integer('tokens_used').notNull().default(0),
    /** Retry count */
    attemptCount: integer('attempt_count').notNull().default(0),
    /** Worker heartbeat time (used in M2) */
    lastHeartbeat: timestamp('last_heartbeat', { withTimezone: true }),
    /** Execution duration (milliseconds) */
    durationMs: integer('duration_ms'),
    /** Error message */
    errorMessage: text('error_message'),
    /** Recording configuration */
    config: jsonb('config').$type<RecordingConfig>(),
    /** Start time */
    startedAt: timestamp('started_at', { withTimezone: true }),
    /** Completion time */
    completedAt: timestamp('completed_at', { withTimezone: true }),
    /** Created time */
    createdAt: timestamp('created_at', { withTimezone: true }).notNull().defaultNow(),
    /** Updated time */
    updatedAt: timestamp('updated_at', { withTimezone: true }).notNull().defaultNow(),
  },
  (table) => ({
    // Unique constraint: one recording task per (chunk_id, build_task_id) combination
    // This allows the same chunk to be processed in different build tasks
    uniqueChunkBuildTask: unique('recording_tasks_chunk_build_task_unique').on(
      table.chunkId,
      table.buildTaskId
    ),
  })
);

// ============================================================================
// JSON Types
// ============================================================================

/**
 * RecordingTaskStatus - Recording task status
 */
export type RecordingTaskStatus =
  | 'pending'    // Pending execution
  | 'running'    // Executing
  | 'completed'  // Completed
  | 'failed';    // Execution failed

/**
 * RecordingConfig - Recording configuration
 */
export interface RecordingConfig {
  maxPages?: number;
  timeout?: number;
  headless?: boolean;
  modelName?: string;
  chunk_type?: 'task_driven' | 'exploratory';  // Dual-mode prompt type
  [key: string]: unknown;
}

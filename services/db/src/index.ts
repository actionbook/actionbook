// ============================================================================
// @actionbookdev/db - Shared Database Package
// ============================================================================

// Database connection
export { createDb, getDb, closeDb, type Database } from './connection'

// Re-export drizzle-orm operators for convenience
export {
  eq,
  and,
  or,
  sql,
  isNull,
  isNotNull,
  inArray,
  desc,
  asc,
  ilike,
  like,
  lt,
  gt,
  lte,
  gte,
} from 'drizzle-orm'

// Schema (tables) - Crawl
export { sources, sourceVersions, documents, chunks, crawlLogs } from './schema'

// Schema (tables) - Auth
export { apiKeys } from './schema'

// Schema (tables) - Action Builder
export { pages, elements, recordingTasks, recordingSteps } from './schema'

// Schema (tables) - Build Pipeline
export { buildTasks } from './schema'

// Types inferred from schema - Crawl
export type {
  Source,
  NewSource,
  SourceVersion,
  NewSourceVersion,
  Document,
  NewDocument,
  Chunk,
  NewChunk,
  CrawlLog,
  NewCrawlLog,
} from './types'

// Types inferred from schema - Action Builder
export type {
  Page,
  NewPage,
  Element,
  NewElement,
  RecordingTask,
  NewRecordingTask,
  RecordingStep,
  NewRecordingStep,
} from './types'

// Types inferred from schema - Build Pipeline
export type { BuildTask, NewBuildTask } from './types'

// JSON column types - Crawl
export type {
  CrawlConfig,
  SourceVersionStatus,
  DocumentStatus,
  BreadcrumbItem,
  HeadingItem,
  CrawlStatus,
  CrawlError,
} from './types'

// JSON column types - Playbook Builder
export type { ActionCategory, ActionStatus } from './types'

// JSON column types - Action Builder
export type {
  ElementType,
  ElementStatus,
  AllowMethod,
  ArgumentDef,
  RecordingTaskStatus,
  RecordingConfig,
  ToolName,
  RecordingStepStatus,
  SelectorType,
  SelectorItem,
  TemplateParam,
} from './types'

// JSON column types - Build Pipeline
export type {
  SourceCategory,
  BuildTaskStage,
  BuildTaskStageStatus,
  BuildTaskConfig,
} from './types'

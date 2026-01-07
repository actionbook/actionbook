import type { InferSelectModel, InferInsertModel } from 'drizzle-orm';
import {
  sources,
  sourceVersions,
  documents,
  chunks,
  crawlLogs,
  pages,
  elements,
  recordingTasks,
  recordingSteps,
  buildTasks,
} from './models';

// ============================================================================
// Inferred Types from Schema - Crawl
// ============================================================================

// Source
export type Source = InferSelectModel<typeof sources>;
export type NewSource = InferInsertModel<typeof sources>;

// SourceVersion
export type SourceVersion = InferSelectModel<typeof sourceVersions>;
export type NewSourceVersion = InferInsertModel<typeof sourceVersions>;

// Document
export type Document = InferSelectModel<typeof documents>;
export type NewDocument = InferInsertModel<typeof documents>;

// Chunk
export type Chunk = InferSelectModel<typeof chunks>;
export type NewChunk = InferInsertModel<typeof chunks>;

// CrawlLog
export type CrawlLog = InferSelectModel<typeof crawlLogs>;
export type NewCrawlLog = InferInsertModel<typeof crawlLogs>;

// ============================================================================
// Inferred Types from Schema - Action Builder
// ============================================================================

// Page
export type Page = InferSelectModel<typeof pages>;
export type NewPage = InferInsertModel<typeof pages>;

// Element
export type Element = InferSelectModel<typeof elements>;
export type NewElement = InferInsertModel<typeof elements>;

// RecordingTask
export type RecordingTask = InferSelectModel<typeof recordingTasks>;
export type NewRecordingTask = InferInsertModel<typeof recordingTasks>;

// RecordingStep
export type RecordingStep = InferSelectModel<typeof recordingSteps>;
export type NewRecordingStep = InferInsertModel<typeof recordingSteps>;

// ============================================================================
// Inferred Types from Schema - Build Pipeline
// ============================================================================

// BuildTask
export type BuildTask = InferSelectModel<typeof buildTasks>;
export type NewBuildTask = InferInsertModel<typeof buildTasks>;

// ============================================================================
// Re-export JSON types from models - Crawl
// ============================================================================
export type {
  CrawlConfig,
  SourceVersionStatus,
  DocumentStatus,
  BreadcrumbItem,
  HeadingItem,
  CrawlStatus,
  CrawlError,
} from './models';

// ============================================================================
// Re-export JSON types from models - Playbook Builder
// ============================================================================
export type { ActionCategory, ActionStatus } from './models';

// ============================================================================
// Re-export JSON types from models - Action Builder
// ============================================================================
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
} from './models';

// ============================================================================
// Re-export JSON types from models - Build Pipeline
// ============================================================================
export type {
  SourceCategory,
  BuildTaskStage,
  BuildTaskStageStatus,
  BuildTaskConfig,
} from './models';

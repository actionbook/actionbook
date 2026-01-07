// ============================================================================
// Schema - Unified export of all tables and types
// ============================================================================

// Tables - Crawl
export {
  sources,
  sourceVersions,
  documents,
  chunks,
  crawlLogs,
} from './models';

// Tables - Action Builder
export {
  pages,
  elements,
  recordingTasks,
  recordingSteps,
} from './models';

// Tables - Auth
export { apiKeys } from './models';

// Tables - Build Pipeline
export { buildTasks } from './models';

// JSON types - Crawl
export type {
  CrawlConfig,
  SourceVersionStatus,
  DocumentStatus,
  BreadcrumbItem,
  HeadingItem,
  ActionCategory,
  ActionStatus,
  CrawlStatus,
  CrawlError,
} from './models';

// JSON types - Action Builder
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

// JSON types - Build Pipeline
export type {
  SourceCategory,
  BuildTaskStage,
  BuildTaskStageStatus,
  BuildTaskConfig,
} from './models';

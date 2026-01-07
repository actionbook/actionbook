// ============================================================================
// Models - Database table definitions
// ============================================================================

// Tables - Crawl
export { sources } from './source';
export { sourceVersions } from './source-version';
export { documents } from './document';
export { chunks } from './chunk';
export { crawlLogs } from './crawl-log';

// Tables - Auth
export { apiKeys } from './api-key';

// Tables - Action Builder
export { pages } from './page';
export { elements } from './element';
export { recordingTasks } from './recording-task';
export { recordingSteps } from './recording-step';

// Tables - Build Pipeline
export { buildTasks } from './build-task';

// JSON types - Crawl
export type { CrawlConfig } from './source';
export type { SourceVersionStatus } from './source-version';
export type { DocumentStatus, BreadcrumbItem } from './document';
export type { HeadingItem, ActionCategory, ActionStatus } from './chunk';
export type { CrawlStatus, CrawlError } from './crawl-log';

// JSON types - Action Builder
export type {
  ElementType,
  ElementStatus,
  AllowMethod,
  ArgumentDef,
  SelectorType,
  SelectorItem,
  TemplateParam,
} from './element';
export type { RecordingTaskStatus, RecordingConfig } from './recording-task';
export type { ToolName, RecordingStepStatus } from './recording-step';

// JSON types - Build Pipeline
export type {
  SourceCategory,
  BuildTaskStage,
  BuildTaskStageStatus,
  BuildTaskConfig,
} from './build-task';

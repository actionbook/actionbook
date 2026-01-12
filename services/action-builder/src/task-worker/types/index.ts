/**
 * Task Worker Types
 *
 * Defines core types for Task Worker module
 */

/**
 * Task Status
 */
export type TaskStatus = 'pending' | 'running' | 'completed' | 'failed';

/**
 * Chunk Type (dual-mode Prompt)
 */
export type ChunkType = 'task_driven' | 'exploratory';

/**
 * Chunk Data (read from recording_chunks table)
 */
export interface ChunkData {
  id: string;
  source_id: string;
  document_url: string;
  document_title: string;
  source_domain: string;
  chunk_content: string;
  chunk_index: number;
  created_at: Date;
  /** App/Product URL for action building (optional, LLM will infer if not set) */
  source_app_url?: string;
}

/**
 * Task Configuration
 */
export interface TaskConfig {
  chunk_type: ChunkType;
  max_retries?: number;
  timeout?: number;
  /**
   * Custom prompt for action builder optimization
   * Appended to the user prompt to provide site-specific instructions
   * Example: "Focus on search functionality and ignore promotional banners"
   */
  actionBuilderPrompt?: string;
}

/**
 * Recording Task (read from recording_tasks table)
 */
export interface RecordingTask {
  id: number;
  sourceId: number;
  chunkId: number | null;
  startUrl: string;
  status: TaskStatus;
  progress: number;
  config: TaskConfig;
  attemptCount: number;
  errorMessage?: string | null;
  completedAt?: Date | null;
  lastHeartbeat?: Date | null;
  createdAt: Date;
  updatedAt: Date;
}

/**
 * Task Execution Result
 */
export interface ExecutionResult {
  success: boolean;
  actions_created: number;
  error?: string;
  duration_ms: number;
  turns?: number;           // LLM conversation turns
  tokens_used?: number;     // Token consumption
  saved_path?: string;      // YAML save path
}

/**
 * TaskExecutor Configuration
 *
 * LLM configuration is auto-detected from environment variables:
 *   - OPENROUTER_API_KEY (priority 1)
 *   - OPENAI_API_KEY (priority 2)
 *   - ANTHROPIC_API_KEY (priority 3)
 */
export interface TaskExecutorConfig {
  /** @deprecated Use environment variables for API key auto-detection */
  llmApiKey?: string;
  /** @deprecated Use environment variables for base URL */
  llmBaseURL?: string;
  /** @deprecated Use environment variables for model selection */
  llmModel?: string;
  databaseUrl: string;
  headless?: boolean;       // Default true (headless mode)
  maxTurns?: number;        // Default 30
  outputDir?: string;       // YAML output directory
  /** Enable browser profile for persistent login state */
  profileEnabled?: boolean; // Default false
  /** Profile directory path */
  profileDir?: string;      // Default '.browser-profile'
  /** Task execution timeout in minutes (default: 10) */
  taskTimeoutMinutes?: number;
}

/**
 * Source Information (for ActionBuilder)
 */
export interface SourceInfo {
  id: number;
  name: string;
  domain: string;
  baseUrl: string;
  description: string | null;
}

/**
 * Prompt Build Result
 */
export interface PromptResult {
  systemPrompt: string;
  userPrompt: string;
  chunkType: ChunkType;
}

// ============================================================================
// Build Task Types (for BuildTaskWorker)
// ============================================================================

/**
 * Build Task Stage
 */
export type BuildTaskStage =
  | 'init'
  | 'knowledge_build'
  | 'action_build'
  | 'completed'
  | 'error';

/**
 * Build Task Stage Status
 */
export type BuildTaskStageStatus = 'pending' | 'running' | 'completed' | 'error';

/**
 * Source Category
 */
export type SourceCategory = 'help' | 'playbook' | 'unknown' | 'any';

/**
 * Build Task Config (stored in JSONB)
 */
export interface BuildTaskConfig {
  maxDepth?: number;
  includePatterns?: string[];
  excludePatterns?: string[];
  rateLimit?: number;
  attemptCount?: number;
  lastError?: string;
  stats?: BuildTaskStats;
  [key: string]: unknown;
}

/**
 * Build Task Info (from build_tasks table)
 */
export interface BuildTaskInfo {
  id: number;
  sourceId: number | null;
  sourceVersionId: number | null; // 1:1 relationship with source_version
  sourceUrl: string;
  sourceName: string | null;
  sourceCategory: SourceCategory;
  stage: BuildTaskStage;
  stageStatus: BuildTaskStageStatus;
  config: BuildTaskConfig;
  knowledgeStartedAt: Date | null;
  knowledgeCompletedAt: Date | null;
  actionStartedAt: Date | null;
  actionCompletedAt: Date | null;
  createdAt: Date;
  updatedAt: Date;
}

/**
 * Build Task Execution Statistics
 */
export interface BuildTaskStats {
  recordingTasksReset: number;
  recordingTasksCreated: number;
  recordingTasksCompleted: number;
  recordingTasksFailed: number;
  elementsCreated: number;
  duration_ms: number;
}

/**
 * Build Task Worker Configuration
 */
export interface BuildTaskWorkerConfig extends TaskExecutorConfig {
  pollInterval?: number;       // Default: 30000ms (30 seconds)
  recordingTaskLimit?: number; // Default: 100
  maxAttempts?: number;        // Default: 3
  staleTimeoutMinutes?: number; // Default: 30 (tasks running > 30min are considered stale)
  heartbeatIntervalMs?: number; // Default: 30000ms (30 seconds) - interval for heartbeat updates
  concurrency?: number;        // Default: 1 (sequential), set > 1 for concurrent execution
  // taskTimeoutMinutes is inherited from TaskExecutorConfig
}

/**
 * Build Task Execution Result
 */
export interface BuildTaskResult {
  success: boolean;
  taskId: number;
  recordingTasksReset: number;
  recordingTasksCreated: number;
  recordingTasksCompleted: number;
  recordingTasksFailed: number;
  elementsCreated: number;
  duration_ms: number;
  error?: string;
  /** Published version ID (only set on successful completion) */
  publishedVersionId?: number;
}

/**
 * Build Task Scheduler Configuration
 */
export interface BuildTaskSchedulerConfig {
  maxAttempts?: number; // Default: 3
  staleTimeoutMinutes?: number; // Default: 30 (tasks running > 30min are considered stale)
}

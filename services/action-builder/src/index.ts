// Main exports
export { ActionBuilder, createActionBuilder } from "./ActionBuilder.js";

// Sub-modules
export { StagehandBrowser, ElementNotFoundError, ActionExecutionError } from "@actionbookdev/browser";
export type { BrowserAdapter } from "@actionbookdev/browser";
export { AIClient } from "./llm/index.js";
export { ActionRecorder } from "./recorder/index.js";
export { SelectorValidator } from "./validator/index.js";
export { YamlWriter } from "./writers/index.js";

// Prompts
export {
  CAPABILITY_RECORDER_SYSTEM_PROMPT,
  generateUserPrompt,
} from "./llm/prompts/capability-recorder.js";

// Types
export type {
  // Capability types
  ArgumentDef,
  ElementType,
  AllowMethod,
  ActionMethod,
  ActionObject,
  ElementCapability,
  PageCapability,
  SiteCapability,
  ObserveResultItem,
  // Config types
  ActionBuilderConfig,
  BrowserConfig,
  LLMConfig,
  RecorderConfig,
  ValidatorConfig,
  BuildOptions,
  BuildResult,
  RecordResult,
  ElementValidationResult,
  ValidationResult,
  // Step event types (for real-time feedback)
  StepEvent,
  OnStepFinishCallback,
} from "./types/index.js";

// Utils
export { log, logRaw, fileLogger } from "./utils/logger.js";
export { withRetry, sleep, humanDelay } from "./utils/retry.js";
export { truncate, formatToolResult } from "./utils/string.js";

/**
 * Controller Layer - Entry point
 *
 * Provides BuildTaskController for polling and executing knowledge-builder tasks
 */

// Factory and implementation
export {
  createBuildTaskController,
  BuildTaskControllerImpl,
} from './build-task-controller.js';

// Task mapper utilities
export { mapTaskToProcessorConfig, validateTask } from './task-mapper.js';

// Types
export type {
  BuildTaskController,
  ControllerOptions,
  ControllerState,
  BuildTask,
  BuildTaskConfig,
} from './types.js';

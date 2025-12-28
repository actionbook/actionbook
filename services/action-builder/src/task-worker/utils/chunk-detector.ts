/**
 * Chunk Type Detector
 *
 * Auto-detect chunk content type: task_driven or exploratory
 */

import type { ChunkType } from '../types/index.js';

/**
 * Detect chunk content type
 *
 * Rules:
 * - Contains step numbers (1. 2. 3.) → task_driven
 * - Contains keywords (Steps, Task, Scenario) → task_driven
 * - Otherwise → exploratory
 *
 * @param content - chunk content
 * @returns chunk type
 */
export function detectChunkType(content: string): ChunkType {
  // Rule 1: Detect step numbers (line-start digit + dot)
  const hasStepNumbers = /^\s*\d+\.\s+/m.test(content);

  // Rule 2: Detect task-driven keywords (case-insensitive, word boundary)
  const hasTaskKeywords = /\b(steps|task|scenario|workflow|execute|perform)\b/i.test(content);

  // Return task_driven or exploratory
  return (hasStepNumbers || hasTaskKeywords) ? 'task_driven' : 'exploratory';
}

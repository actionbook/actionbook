/**
 * Mock Factory - Test Mock Utilities
 *
 * Provides unified Mock creation functions
 */

import { vi } from 'vitest';

/**
 * Create Mock Database
 */
export function createMockDatabase() {
  return {
    select: vi.fn().mockReturnThis(),
    from: vi.fn().mockReturnThis(),
    where: vi.fn().mockReturnThis(),
    orderBy: vi.fn().mockReturnThis(),
    limit: vi.fn().mockReturnThis(),
    insert: vi.fn(),
    update: vi.fn(),
    execute: vi.fn(),
  };
}

/**
 * Create Mock ActionBuilder
 */
export function createMockActionBuilder() {
  return {
    build: vi.fn(),
    close: vi.fn(),
  };
}

/**
 * Create Mock BuildTaskScheduler
 */
export function createMockBuildTaskScheduler() {
  return {
    claimNextActionTask: vi.fn(),
    getNextActionTask: vi.fn(),
    startActionStage: vi.fn(),
    completeTask: vi.fn(),
    failTask: vi.fn(),
    getTaskById: vi.fn(),
    updateHeartbeat: vi.fn(),
    publishVersion: vi.fn().mockResolvedValue({ success: true, versionId: 1 }),
  };
}

/**
 * Create Mock TaskGenerator
 */
export function createMockTaskGenerator() {
  return {
    generate: vi.fn(),
  };
}

/**
 * Create Mock TaskScheduler
 */
export function createMockTaskScheduler() {
  return {
    getNextTask: vi.fn(),
    getNextTaskWithRecovery: vi.fn(),
    claimNextTask: vi.fn(),
    markRunning: vi.fn(),
    markCompleted: vi.fn(),
    markFailed: vi.fn(),
    updateHeartbeat: vi.fn(),
  };
}

/**
 * Create Mock TaskExecutor
 */
export function createMockTaskExecutor() {
  return {
    execute: vi.fn(),
  };
}

/**
 * Create Mock WorkerPool
 */
export function createMockWorkerPool() {
  return {
    executeAll: vi.fn(),
    shutdown: vi.fn(),
  };
}

/**
 * Create sample BuildTaskInfo for testing
 */
export function createSampleBuildTaskInfo(overrides: Partial<{
  id: number;
  sourceId: number | null;
  sourceUrl: string;
  sourceName: string | null;
  sourceCategory: 'help' | 'unknown';
  stage: string;
  stageStatus: string;
  config: Record<string, unknown>;
  knowledgeStartedAt: Date | null;
  knowledgeCompletedAt: Date | null;
  actionStartedAt: Date | null;
  actionCompletedAt: Date | null;
  createdAt: Date;
  updatedAt: Date;
}> = {}) {
  const now = new Date();
  return {
    id: 1,
    sourceId: 100,
    sourceUrl: 'https://help.example.com',
    sourceName: 'Example Help Center',
    sourceCategory: 'help' as const,
    stage: 'knowledge_build',
    stageStatus: 'completed',
    config: {},
    knowledgeStartedAt: now,
    knowledgeCompletedAt: now,
    actionStartedAt: null,
    actionCompletedAt: null,
    createdAt: now,
    updatedAt: now,
    ...overrides,
  };
}

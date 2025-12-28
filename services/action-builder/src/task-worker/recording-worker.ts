/**
 * RecordingWorker - Recording Task Worker
 *
 * M1 version: Single-run mode (no heartbeat mechanism)
 */

import type { TaskScheduler } from './task-scheduler.js';
import type { TaskExecutor } from './task-executor.js';

export class RecordingWorker {
  constructor(
    private scheduler: TaskScheduler,
    private executor: TaskExecutor
  ) {}

  /**
   * Run one complete cycle (M1: execute 10 tasks)
   */
  async runOnce(limit: number = 10): Promise<void> {
    for (let i = 0; i < limit; i++) {
      const task = await this.scheduler.getNextTask();
      if (!task) {
        return;
      }

      try {
        await this.executor.execute(task);
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        // Best-effort: mark task failed (TaskExecutor may also update status)
        await this.scheduler.markFailed(task.id, errorMessage);
      }
    }
  }
}

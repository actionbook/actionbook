#!/usr/bin/env node
/**
 * Playbook Builder Worker
 *
 * CLI entry point for running the PlaybookTaskController as a worker process.
 * Polls the database for pending playbook tasks and executes them.
 *
 * Usage:
 *   pnpm dev
 */

import 'dotenv/config';
import { createPlaybookTaskController, type PlaybookTaskController } from '../src/controller/index.js';

/**
 * Setup graceful shutdown handlers
 */
function setupShutdownHandlers(controller: PlaybookTaskController): void {
  let isShuttingDown = false;

  const shutdown = async (signal: string) => {
    if (isShuttingDown) {
      console.log(`\n[Worker] Already shutting down, please wait...`);
      return;
    }

    isShuttingDown = true;
    console.log(`\n[Worker] Received ${signal}, initiating graceful shutdown...`);

    try {
      await controller.stop(`Received signal ${signal}, do graceful shutdown`);
      console.log('[Worker] Shutdown complete');
      process.exit(0);
    } catch (error) {
      console.error('[Worker] Error during shutdown:', error);
      process.exit(1);
    }
  };

  process.on('SIGINT', () => shutdown('SIGINT'));
  process.on('SIGTERM', () => shutdown('SIGTERM'));
}

async function main() {
  console.log('========================================');
  console.log('  Playbook Builder Worker');
  console.log('========================================');
  console.log('');

  const controller = createPlaybookTaskController();

  const controllerOptions = {
    onTaskStart: (taskId: number) => {
      console.log(`\n[Worker] Task #${taskId} started`);
    },
    onTaskComplete: (taskId: number, result: { playbookCount: number; actionCount: number; durationMs: number }) => {
      const duration = (result.durationMs / 1000).toFixed(1);
      console.log(`\n[Worker] Task #${taskId} completed: ${result.playbookCount} playbooks, ${result.actionCount} actions in ${duration}s`);
    },
    onTaskError: (taskId: number, error: Error, retryCount: number) => {
      console.error(`\n[Worker] Task #${taskId} error (attempt ${retryCount}): ${error.message}`);
    },
  };

  console.log('[Worker] Starting continuous polling mode');
  console.log('[Worker] Looking for tasks with:');
  console.log('  - stage=init, status=pending');
  console.log('  - OR stage=knowledge_build, status=pending (retry tasks)');
  console.log('');
  console.log('Press Ctrl+C to stop gracefully');
  console.log('');

  // Setup graceful shutdown handlers
  setupShutdownHandlers(controller);

  await controller.start(controllerOptions);
}

main().catch((error) => {
  console.error('[Worker] Fatal error:', error);
  process.exit(1);
});

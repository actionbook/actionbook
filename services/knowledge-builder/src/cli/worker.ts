#!/usr/bin/env node
/**
 * Knowledge Builder Worker
 *
 * CLI entry point for running the BuildTaskController as a worker process.
 * Polls the database for pending build tasks and executes them.
 *
 * Usage:
 *   pnpm dev
 */

import 'dotenv/config';
import { createBuildTaskController, type BuildTaskController } from '../controller/index.js';

/**
 * Setup graceful shutdown handlers
 */
function setupShutdownHandlers(controller: BuildTaskController): void {
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
  console.log('  Knowledge Builder Worker');
  console.log('========================================');
  console.log('');

  const controller = createBuildTaskController();

  const controllerOptions = {
    onTaskStart: (taskId: number) => {
      console.log(`\n[Worker] Task #${taskId} started`);
    },
    onTaskComplete: (taskId: number, result: { totalPages: number; durationMs: number }) => {
      const duration = (result.durationMs / 1000).toFixed(1);
      console.log(`\n[Worker] Task #${taskId} completed: ${result.totalPages} pages in ${duration}s`);
    },
    onTaskError: (taskId: number, error: Error, retryCount: number) => {
      console.error(`\n[Worker] Task #${taskId} error (attempt ${retryCount}): ${error.message}`);
    },
    onProgress: (taskId: number, progress: { phase: string; pagesProcessed: number; currentUrl?: string }) => {
      if (progress.currentUrl) {
        console.log(`[Worker] Task #${taskId} [${progress.phase}] ${progress.pagesProcessed} pages - ${progress.currentUrl}`);
      }
    },
  };

  console.log('[Worker] Starting continuous polling mode');
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

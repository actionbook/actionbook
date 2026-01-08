#!/usr/bin/env tsx
/**
 * Coordinator Entry Script
 *
 * Start Task Queue Architecture coordinator
 * Manage build_task concurrent execution and recording_task global queue
 */

import { getDb } from '@actionbookdev/db';
import { Coordinator } from '../src/task-worker/coordinator';

async function main() {
  console.log('='.repeat(60));
  console.log('Task Queue Coordinator');
  console.log('='.repeat(60));

  const db = getDb();

  // Read configuration (reuse existing environment variable names, time units use suffixes)
  const config = {
    maxConcurrentBuildTasks: parseInt(
      process.env.ACTION_BUILDER_MAX_CONCURRENT_BUILD_TASKS ?? '5'
    ),
    buildTaskPollIntervalSeconds: parseInt(
      process.env.ACTION_BUILDER_BUILD_TASK_POLL_INTERVAL_SECONDS ?? '5'
    ),
    buildTaskStaleTimeoutMinutes: parseInt(
      process.env.ACTION_BUILDER_BUILD_TASK_STALE_TIMEOUT_MINUTES ?? '15'
    ),
    queueWorker: {
      concurrency: parseInt(process.env.ACTION_BUILDER_TASK_CONCURRENCY ?? '3'),
      staleTimeoutMinutes: parseInt(
        process.env.ACTION_BUILDER_STALE_TIMEOUT_MINUTES ?? '15'
      ),
      taskTimeoutMinutes: parseInt(
        process.env.ACTION_BUILDER_TASK_TIMEOUT_MINUTES ?? '10'
      ),
      databaseUrl: process.env.DATABASE_URL!,
      headless: process.env.ACTION_BUILDER_HEADLESS !== 'false',
      outputDir: process.env.OUTPUT_DIR ?? './output',
      maxAttempts: parseInt(process.env.ACTION_BUILDER_MAX_ATTEMPTS ?? '3'),
      maxTurns: parseInt(process.env.ACTION_BUILDER_MAX_TURNS ?? '30'),
    },
    buildTaskRunner: {
      checkIntervalSeconds: parseInt(
        process.env.ACTION_BUILDER_CHECK_INTERVAL_SECONDS ?? '5'
      ),
      maxAttempts: parseInt(process.env.ACTION_BUILDER_MAX_ATTEMPTS ?? '3'),
    },
  };

  console.log('\nConfiguration:');
  console.log(`  Max Concurrent Build Tasks: ${config.maxConcurrentBuildTasks}`);
  console.log(`  Build Task Poll Interval: ${config.buildTaskPollIntervalSeconds}s`);
  console.log(`  Build Task Stale Timeout: ${config.buildTaskStaleTimeoutMinutes} minutes`);
  console.log(`  Recording Task Concurrency: ${config.queueWorker.concurrency}`);
  console.log(`  Recording Task Stale Timeout: ${config.queueWorker.staleTimeoutMinutes} minutes`);
  console.log(`  Task Timeout: ${config.queueWorker.taskTimeoutMinutes} minutes`);
  console.log(`  Max Attempts: ${config.buildTaskRunner.maxAttempts}`);
  console.log('');

  const coordinator = new Coordinator(db, config);

  // Handle graceful shutdown
  const gracefulShutdown = async (signal: string) => {
    console.log(`\n[${signal}] Received, shutting down gracefully...`);
    await coordinator.stop(60000); // 60 seconds timeout
    process.exit(0);
  };

  process.on('SIGINT', () => gracefulShutdown('SIGINT'));
  process.on('SIGTERM', () => gracefulShutdown('SIGTERM'));

  // Start
  await coordinator.start();
}

main().catch((error) => {
  console.error('Fatal error:', error);
  process.exit(1);
});

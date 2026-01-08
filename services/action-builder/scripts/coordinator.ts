#!/usr/bin/env tsx
/**
 * Coordinator Entry Script
 *
 * 启动 Task Queue Architecture 的协调器
 * 管理 build_task 并发执行和 recording_task 全局队列
 */

import { getDb } from '@actionbookdev/db';
import { Coordinator } from '../src/task-worker/coordinator';

async function main() {
  console.log('='.repeat(60));
  console.log('Task Queue Coordinator');
  console.log('='.repeat(60));

  const db = getDb();

  // 读取配置（复用现有环境变量名）
  const config = {
    maxConcurrentBuildTasks: parseInt(
      process.env.ACTION_BUILDER_MAX_CONCURRENT_BUILD_TASKS ?? '5'
    ),
    buildTaskPollIntervalSeconds: parseInt(
      process.env.ACTION_BUILDER_BUILD_TASK_POLL_INTERVAL ?? '5'
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
  console.log(`  Recording Task Concurrency: ${config.queueWorker.concurrency}`);
  console.log(`  Stale Timeout: ${config.queueWorker.staleTimeoutMinutes} minutes`);
  console.log(`  Task Timeout: ${config.queueWorker.taskTimeoutMinutes} minutes`);
  console.log(`  Max Attempts: ${config.buildTaskRunner.maxAttempts}`);
  console.log('');

  const coordinator = new Coordinator(db, config);

  // 处理优雅关闭
  const gracefulShutdown = async (signal: string) => {
    console.log(`\n[${signal}] Received, shutting down gracefully...`);
    await coordinator.stop(60000); // 60 秒超时
    process.exit(0);
  };

  process.on('SIGINT', () => gracefulShutdown('SIGINT'));
  process.on('SIGTERM', () => gracefulShutdown('SIGTERM'));

  // 启动
  await coordinator.start();
}

main().catch((error) => {
  console.error('Fatal error:', error);
  process.exit(1);
});

import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    globals: true,
    environment: 'node',
    include: ['test/**/*.e2e.test.ts'],
    exclude: ['node_modules'],
    // E2E tests need longer timeout (browser operations + AI calls)
    testTimeout: 120000,
    // Run tests sequentially to avoid resource contention
    fileParallelism: false,
    pool: 'threads',
    poolOptions: {
      threads: { singleThread: true },
    },
  },
});

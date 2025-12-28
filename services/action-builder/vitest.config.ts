import { defineConfig } from 'vitest/config'
import { resolve } from 'path'

export default defineConfig({
  test: {
    globals: true,
    environment: 'node',
    // Many tests use a shared real Postgres DB; run in a single worker to avoid cross-test interference.
    fileParallelism: false,
    pool: 'threads',
    poolOptions: {
      threads: { singleThread: true },
    },
    include: [
      'src/**/*.test.ts',
      'test/**/*.ut.test.ts', // Unit tests
      'test/**/*.it.test.ts', // Integration tests
    ],
    exclude: ['node_modules'],
    testTimeout: 30000, // Database tests need more time
    setupFiles: ['./vitest.setup.ts'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'html', 'lcov'],
      include: ['src/task-worker/**/*.ts'],
      exclude: ['src/task-worker/**/*.test.ts', 'src/task-worker/types/**'],
      branches: 70, // M1 target: branch coverage ≥70%
      lines: 75, // M1 target: line coverage ≥75%
      functions: 80, // M1 target: function coverage ≥80%
    },
  },
  resolve: {
    alias: {
      '@actionbookdev/db': resolve(__dirname, '../db/dist/index.js'),
    },
  },
})

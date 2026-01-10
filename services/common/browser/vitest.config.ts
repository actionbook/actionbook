import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    globals: true,
    environment: 'node',
    include: ['test/**/*.ut.test.ts'],
    exclude: ['node_modules', 'test/**/*.e2e.test.ts'],
    testTimeout: 30000,
  },
});

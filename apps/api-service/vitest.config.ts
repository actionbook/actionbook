import { defineConfig } from 'vitest/config';
import path from 'path';
import dotenv from 'dotenv';

// Load environment variables - First load db package .env, then load local .env (override)
dotenv.config({ path: '../db/.env' });
dotenv.config({ path: '.env' });

export default defineConfig({
  test: {
    environment: 'node',
    globals: true,
    testTimeout: 60000,      // Increased to 60 seconds because embedding API may be slow
    hookTimeout: 60000,      // Hook timeout
    include: ['test/**/*.test.ts'],
    // Environment variables have been loaded into process.env via dotenv.config,
    // Vitest runs in the same process and can usually access process.env directly.
    // But to ensure, we can also inject explicitly via env property
    // Filter out undefined values, keep only string type environment variables
    env: Object.fromEntries(
      Object.entries(process.env).filter(([_, value]) => value !== undefined)
    ) as Record<string, string>,
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './'),
    },
  },
});

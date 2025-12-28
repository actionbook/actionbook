import { config } from 'dotenv';
import { resolve } from 'path';

// Load .env file from services/db (which has DATABASE_URL)
config({ path: resolve(__dirname, '../db/.env') });

// Also load local .env if exists
config({ path: resolve(__dirname, '.env'), override: true });

// Prefer the e2e DB URL if that's what is configured locally.
if (process.env.ACTION_BUILDER_E2E_DATABASE_URL) {
  process.env.DATABASE_URL = process.env.ACTION_BUILDER_E2E_DATABASE_URL;
}

import { drizzle } from 'drizzle-orm/node-postgres';
import { Pool } from 'pg';
import * as schema from './schema';

// Store pool reference for closing
const poolMap = new WeakMap<ReturnType<typeof drizzle>, Pool>();

/**
 * Create a database connection using the DATABASE_URL environment variable.
 * Uses node-postgres (pg) driver for local/standard PostgreSQL.
 */
export function createDb(databaseUrl?: string) {
  const url = databaseUrl ?? process.env.DATABASE_URL;
  if (!url) {
    throw new Error('DATABASE_URL environment variable is required');
  }

  // Enable SSL if:
  // 1. NODE_ENV is production, OR
  // 2. URL contains sslmode parameter, OR
  // 3. URL is not localhost/127.0.0.1
  const isLocalhost = url.includes('localhost') || url.includes('127.0.0.1');
  const hasSslParam = url.includes('sslmode=');
  const isProduction = process.env.NODE_ENV === 'production';
  const needsSsl = isProduction || hasSslParam || !isLocalhost;

  const pool = new Pool({
    connectionString: url,
    // Enable SSL for remote databases, skip certificate verification
    ssl: needsSsl
      ? {
          rejectUnauthorized: false,
          // Additional SSL options for compatibility with cloud databases
          checkServerIdentity: () => undefined,
        }
      : false,
  });
  const db = drizzle(pool, { schema });
  poolMap.set(db, pool);
  return db;
}

/**
 * Close a database connection and release the pool.
 */
export async function closeDb(db: ReturnType<typeof createDb>): Promise<void> {
  const pool = poolMap.get(db);
  if (pool) {
    await pool.end();
    poolMap.delete(db);
  }
}

/**
 * Default database instance.
 * Lazily initialized on first access.
 */
let _db: ReturnType<typeof createDb> | null = null;

export function getDb() {
  if (!_db) {
    _db = createDb();
  }
  return _db;
}

export type Database = ReturnType<typeof createDb>;

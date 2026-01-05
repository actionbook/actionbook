import { drizzle as drizzlePg } from 'drizzle-orm/node-postgres';
import { drizzle as drizzleNeon } from 'drizzle-orm/neon-http';
import { neon } from '@neondatabase/serverless';
import { Pool } from 'pg';
import * as schema from './schema';

// Store pool reference for closing (only used for pg connections)
const poolMap = new WeakMap<ReturnType<typeof drizzlePg>, Pool>();

/**
 * Check if running in local environment.
 * Local = not production AND not on Vercel.
 */
function isLocalEnv(): boolean {
  return process.env.NODE_ENV !== 'production' && !process.env.VERCEL;
}

/**
 * Create a database connection.
 *
 * Connection strategy:
 * - Local environment + DATABASE_URL: Use node-postgres (pg) driver
 * - Otherwise (Vercel/production): Use Neon serverless driver with POSTGRES_URL
 */
export function createDb(databaseUrl?: string) {
  const isLocal = isLocalEnv();
  const localDbUrl = databaseUrl ?? process.env.DATABASE_URL;

  if (isLocal && localDbUrl) {
    // Local environment with DATABASE_URL: use node-postgres
    return createPgDb(localDbUrl);
  }

  // Production/Vercel: use Neon serverless
  const neonUrl = process.env.POSTGRES_URL;
  if (!neonUrl) {
    throw new Error(
      'Database URL not found. Set DATABASE_URL for local or POSTGRES_URL for Neon.'
    );
  }
  return createNeonDb(neonUrl);
}

/**
 * Create a PostgreSQL connection using node-postgres driver.
 * Used for local development.
 */
function createPgDb(url: string) {
  const isLocalhost = url.includes('localhost') || url.includes('127.0.0.1');
  const hasSslParam = url.includes('sslmode=');
  const needsSsl = hasSslParam || !isLocalhost;

  const pool = new Pool({
    connectionString: url,
    ssl: needsSsl ? { rejectUnauthorized: false } : false,
  });
  const db = drizzlePg(pool, { schema });
  poolMap.set(db, pool);
  return db;
}

/**
 * Create a Neon serverless connection.
 * Used for Vercel/production environment.
 */
function createNeonDb(url: string) {
  const sql = neon(url);
  return drizzleNeon(sql, { schema });
}

/**
 * Close a database connection and release the pool.
 * Only works for pg connections; Neon connections are stateless.
 */
export async function closeDb(db: ReturnType<typeof createDb>): Promise<void> {
  const pool = poolMap.get(db as ReturnType<typeof drizzlePg>);
  if (pool) {
    await pool.end();
    poolMap.delete(db as ReturnType<typeof drizzlePg>);
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

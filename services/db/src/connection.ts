import { drizzle as drizzlePg, type NodePgDatabase } from 'drizzle-orm/node-postgres';
import { drizzle as drizzleNeon } from 'drizzle-orm/neon-serverless';
import { Pool as NeonPool } from '@neondatabase/serverless';
import { Pool } from 'pg';
import * as schema from './schema';

/**
 * Unified database type for both local (pg) and serverless (neon) connections.
 * Both drivers return compatible types when using Pool mode.
 */
export type Database = NodePgDatabase<typeof schema>;

// Store pool references for each database instance
const poolMap = new WeakMap<Database, Pool | NeonPool>();

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
export function createDb(databaseUrl?: string): Database {
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
function createPgDb(url: string): Database {
  const isLocalhost = url.includes('localhost') || url.includes('127.0.0.1');
  const hasSslParam = url.includes('sslmode=');
  const needsSsl = hasSslParam || !isLocalhost;

  const pool = new Pool({
    connectionString: url,
    ssl: needsSsl ? { rejectUnauthorized: false } : false,
  });

  // Add error handler to prevent unhandled error events from crashing the process
  pool.on('error', (err) => {
    console.error('[Database Pool Error]', err.message);
    // Don't throw - let individual query errors be handled by their callers
  });

  const db = drizzlePg(pool, { schema });
  poolMap.set(db, pool);
  return db;
}

/**
 * Create a Neon serverless connection using WebSocket Pool.
 * Used for Vercel/production environment.
 * Using Pool mode for type compatibility with node-postgres.
 */
function createNeonDb(url: string): Database {
  const pool = new NeonPool({ connectionString: url });

  // Add error handler to prevent unhandled error events from crashing the process
  pool.on('error', (err) => {
    console.error('[Database Pool Error]', err.message);
    // Don't throw - let individual query errors be handled by their callers
  });

  // drizzle-orm/neon-serverless with Pool returns compatible type
  const db = drizzleNeon(pool, { schema }) as unknown as Database;
  poolMap.set(db, pool);
  return db;
}

/**
 * Close a database connection and release the pool.
 */
export async function closeDb(db: Database): Promise<void> {
  const pool = poolMap.get(db);
  if (pool) {
    await pool.end();
    poolMap.delete(db);
  }
  // Clear global instance if it matches
  if (_db === db) {
    _db = null;
  }
}

/**
 * Default database instance.
 * Lazily initialized on first access.
 */
let _db: Database | null = null;

export function getDb(): Database {
  if (!_db) {
    _db = createDb();
  }
  return _db;
}

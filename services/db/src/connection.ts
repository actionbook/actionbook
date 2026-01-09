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
 * Check if should use node-postgres driver.
 *
 * Use pg driver for:
 * - Local development (not production)
 * - Node.js environment (e.g.: AgentCore runtime, needs TCP not WebSocket)
 *
 * Use Neon serverless driver only for Vercel (edge/serverless with WebSocket support).
 */
function shouldUsePgDriver(): boolean {
  // Local development
  if (process.env.NODE_ENV !== 'production') {
    return true;
  }
  // AgentCore runtime (Node.js, no native WebSocket)
  if (process.env.AWS_AGENTCORE_RUNTIME === 'true') {
    return true;
  }
  // Non-Vercel production environments use pg driver
  if (!process.env.VERCEL) {
    return true;
  }
  // Vercel uses Neon serverless
  return false;
}

/**
 * Create a database connection.
 *
 * Connection strategy:
 * - Local/AgentCore/Non-Vercel: Use node-postgres (pg) driver
 * - Vercel: Use Neon serverless driver (WebSocket-based)
 */
export function createDb(databaseUrl?: string): Database {
  const usePgDriver = shouldUsePgDriver();
  const dbUrl = databaseUrl ?? process.env.POSTGRES_URL ?? process.env.DATABASE_URL;

  if (!dbUrl) {
    throw new Error(
      'Database URL not found. Set DATABASE_URL or POSTGRES_URL.'
    );
  }

  if (usePgDriver) {
    return createPgDb(dbUrl);
  }

  return createNeonDb(dbUrl);
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

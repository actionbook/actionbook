/**
 * Storage Layer - Data persistence
 *
 * Provides PostgreSQL storage for playbooks and chunks.
 */

export { Storage, createStorage } from './postgres.js';
export type {
  CreatePlaybookInput,
  CreateVersionInput,
  PlaybookResult,
  VersionResult,
} from './types.js';

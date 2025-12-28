import fs from "fs";
import path from "path";
import { log } from "../utils/logger.js";

/**
 * Profile configuration options
 */
export interface ProfileConfig {
  /** Base directory for profile storage, default: '.browser-profile' */
  baseDir?: string;
}

/**
 * Default profile directory path (relative to project root)
 */
export const DEFAULT_PROFILE_DIR = ".browser-profile";

/**
 * BrowserProfileManager - Manages browser profile for persistent login state
 *
 * Uses Playwright's userDataDir feature to persist browser state (cookies, localStorage, etc.)
 * across sessions. This enables "login once, reuse many times" workflow.
 *
 * @example
 * ```typescript
 * const manager = new BrowserProfileManager();
 *
 * // Check if profile exists
 * if (!manager.exists()) {
 *   console.log('Run: pnpm login');
 * }
 *
 * // Get profile path for Stagehand
 * const profilePath = manager.getProfilePath();
 * ```
 */
export class BrowserProfileManager {
  private readonly baseDir: string;

  constructor(config?: ProfileConfig) {
    this.baseDir = config?.baseDir || DEFAULT_PROFILE_DIR;
  }

  /**
   * Get the absolute path to the browser profile directory
   */
  getProfilePath(): string {
    return path.resolve(process.cwd(), this.baseDir);
  }

  /**
   * Check if the browser profile exists
   */
  exists(): boolean {
    const profilePath = this.getProfilePath();
    return fs.existsSync(profilePath);
  }

  /**
   * Ensure the profile directory exists
   */
  ensureDir(): void {
    const profilePath = this.getProfilePath();
    if (!fs.existsSync(profilePath)) {
      fs.mkdirSync(profilePath, { recursive: true });
      log("info", `[BrowserProfileManager] Created profile directory: ${profilePath}`);
    }
  }

  /**
   * Clear the browser profile (delete all data)
   */
  clear(): void {
    const profilePath = this.getProfilePath();
    if (fs.existsSync(profilePath)) {
      fs.rmSync(profilePath, { recursive: true, force: true });
      log("info", `[BrowserProfileManager] Cleared profile directory: ${profilePath}`);
    }
  }

  /**
   * Clean up stale lock files left behind by crashed browser instances
   * Chrome creates SingletonLock to prevent multiple instances from using the same profile.
   * If Chrome crashes or is killed, this file may not be deleted, causing startup issues.
   */
  cleanupStaleLocks(): void {
    const profilePath = this.getProfilePath();
    const lockFiles = ["SingletonLock", "SingletonSocket", "SingletonCookie"];

    for (const lockFile of lockFiles) {
      const lockPath = path.join(profilePath, lockFile);
      if (fs.existsSync(lockPath)) {
        try {
          fs.unlinkSync(lockPath);
          log("info", `[BrowserProfileManager] Cleaned up stale lock file: ${lockFile}`);
        } catch (error) {
          log("warn", `[BrowserProfileManager] Failed to remove ${lockFile}: ${error}`);
        }
      }
    }
  }

  /**
   * Get profile info for display
   */
  getInfo(): { exists: boolean; path: string; size?: string } {
    const profilePath = this.getProfilePath();
    const exists = this.exists();

    if (!exists) {
      return { exists, path: profilePath };
    }

    // Calculate directory size
    let totalSize = 0;
    try {
      const calculateSize = (dir: string): number => {
        let size = 0;
        const files = fs.readdirSync(dir);
        for (const file of files) {
          const filePath = path.join(dir, file);
          const stat = fs.statSync(filePath);
          if (stat.isDirectory()) {
            size += calculateSize(filePath);
          } else {
            size += stat.size;
          }
        }
        return size;
      };
      totalSize = calculateSize(profilePath);
    } catch {
      // Ignore size calculation errors
    }

    // Format size
    const formatSize = (bytes: number): string => {
      if (bytes < 1024) return `${bytes} B`;
      if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
      return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    };

    return {
      exists,
      path: profilePath,
      size: formatSize(totalSize),
    };
  }
}

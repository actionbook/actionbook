import fs from "fs";
import path from "path";

/**
 * Logger function type for BrowserProfileManager
 */
export type ProfileLogger = (
  level: "info" | "warn" | "error" | "debug",
  message: string
) => void;

/**
 * Default console logger
 */
const defaultLogger: ProfileLogger = (level, message) => {
  const prefix = `[BrowserProfileManager]`;
  switch (level) {
    case "error":
      console.error(`${prefix} ${message}`);
      break;
    case "warn":
      console.warn(`${prefix} ${message}`);
      break;
    case "debug":
      // Only log debug in development
      if (process.env.DEBUG) {
        console.debug(`${prefix} ${message}`);
      }
      break;
    default:
      console.log(`${prefix} ${message}`);
  }
};

/**
 * Profile configuration options
 */
export interface ProfileConfig {
  /** Base directory for profile storage, default: '.browser-profile' */
  baseDir?: string;
  /** Custom logger function */
  logger?: ProfileLogger;
}

/**
 * Profile information returned by getInfo()
 */
export interface ProfileInfo {
  exists: boolean;
  path: string;
  size?: string;
}

/**
 * Default profile directory path (relative to project root)
 */
export const DEFAULT_PROFILE_DIR = ".browser-profile";

/**
 * Anti-detection browser arguments for Chromium
 * Use these when launching browser with persistent profile
 */
export const ANTI_DETECTION_ARGS = [
  "--disable-blink-features=AutomationControlled",
  "--no-first-run",
];

/**
 * Default args to ignore when launching with anti-detection
 */
export const IGNORE_DEFAULT_ARGS = ["--enable-automation"];

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
 * // Get profile path for Playwright/Stagehand
 * const profilePath = manager.getProfilePath();
 *
 * // Configure browser launch options
 * const launchOptions = {
 *   userDataDir: profilePath,
 *   preserveUserDataDir: true,
 *   args: ANTI_DETECTION_ARGS,
 *   ignoreDefaultArgs: IGNORE_DEFAULT_ARGS,
 * };
 * ```
 */
export class BrowserProfileManager {
  private readonly baseDir: string;
  private readonly logger: ProfileLogger;

  constructor(config?: ProfileConfig) {
    this.baseDir = config?.baseDir || DEFAULT_PROFILE_DIR;
    this.logger = config?.logger || defaultLogger;
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
      this.logger("info", `Created profile directory: ${profilePath}`);
    }
  }

  /**
   * Clear the browser profile (delete all data)
   */
  clear(): void {
    const profilePath = this.getProfilePath();
    if (fs.existsSync(profilePath)) {
      fs.rmSync(profilePath, { recursive: true, force: true });
      this.logger("info", `Cleared profile directory: ${profilePath}`);
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
          this.logger("info", `Cleaned up stale lock file: ${lockFile}`);
        } catch (error) {
          this.logger("warn", `Failed to remove ${lockFile}: ${error}`);
        }
      }
    }
  }

  /**
   * Get profile info for display
   */
  getInfo(): ProfileInfo {
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

  /**
   * Get browser launch options configured for persistent profile
   * Returns options compatible with Playwright/Stagehand localBrowserLaunchOptions
   */
  getLaunchOptions(options?: {
    headless?: boolean;
    proxy?: { server: string };
  }): Record<string, unknown> {
    this.cleanupStaleLocks();

    const launchOptions: Record<string, unknown> = {
      headless: options?.headless ?? false,
      userDataDir: this.getProfilePath(),
      preserveUserDataDir: true,
      args: ANTI_DETECTION_ARGS,
      ignoreDefaultArgs: IGNORE_DEFAULT_ARGS,
    };

    if (options?.proxy) {
      launchOptions.proxy = options.proxy;
    }

    return launchOptions;
  }
}
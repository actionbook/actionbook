/**
 * Re-export BrowserProfileManager from @actionbookdev/browser-profile
 *
 * This file maintains backward compatibility for existing imports.
 * New code should import directly from "@actionbookdev/browser-profile".
 */

export {
  BrowserProfileManager,
  DEFAULT_PROFILE_DIR,
  ANTI_DETECTION_ARGS,
  IGNORE_DEFAULT_ARGS,
  type ProfileConfig,
  type ProfileInfo,
  type ProfileLogger,
} from "@actionbookdev/browser-profile";
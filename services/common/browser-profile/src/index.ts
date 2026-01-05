/**
 * @actionbookdev/browser-profile
 *
 * Browser profile management for persistent login state.
 * Enables "login once, reuse many times" workflow.
 */

export {
  BrowserProfileManager,
  DEFAULT_PROFILE_DIR,
  ANTI_DETECTION_ARGS,
  IGNORE_DEFAULT_ARGS,
  type ProfileConfig,
  type ProfileInfo,
  type ProfileLogger,
} from "./BrowserProfileManager.js";
import { mkdtempSync, mkdirSync, writeFileSync } from "fs";
import path from "path";
import os from "os";

/**
 * Create an isolated environment with temp directories for HOME, XDG_CONFIG_HOME, XDG_DATA_HOME.
 * This prevents tests from interfering with the user's real config.
 * Matches the Rust cli_test.rs setup_config() pattern.
 *
 * On macOS, pre-seeds the config with --use-mock-keychain in the default
 * profile's extra_args to suppress Keychain permission dialogs during tests.
 */
export function createIsolatedEnv(): {
  tmpDir: string;
  env: Record<string, string>;
} {
  const tmpDir = mkdtempSync(path.join(os.tmpdir(), "actionbook-test-"));
  const home = path.join(tmpDir, "home");
  const configHome = path.join(tmpDir, "config");
  const dataHome = path.join(tmpDir, "data");

  mkdirSync(home, { recursive: true });
  mkdirSync(configHome, { recursive: true });
  mkdirSync(dataHome, { recursive: true });

  // On macOS, Chrome accesses the Keychain for cookie encryption, which
  // triggers permission dialogs in test environments. Pre-seed the config
  // with --use-mock-keychain to suppress these prompts.
  if (process.platform === "darwin") {
    const configDir = path.join(home, "Library", "Application Support", "actionbook");
    mkdirSync(configDir, { recursive: true });
    writeFileSync(
      path.join(configDir, "config.toml"),
      [
        "[profiles.actionbook]",
        "cdp_port = 9222",
        'extra_args = ["--use-mock-keychain"]',
        "",
      ].join("\n")
    );
  }

  return {
    tmpDir,
    env: {
      HOME: home,
      XDG_CONFIG_HOME: configHome,
      XDG_DATA_HOME: dataHome,
    },
  };
}

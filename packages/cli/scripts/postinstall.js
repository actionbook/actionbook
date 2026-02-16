#!/usr/bin/env node

/**
 * Postinstall script for @actionbookdev/cli.
 *
 * Ensures the installed platform binary keeps executable permissions
 * and prints setup instructions for AI agents and CI environments.
 */

"use strict";

const fs = require("fs");
const path = require("path");
const os = require("os");

const PLATFORM_PACKAGES = {
  "darwin-arm64": "@actionbookdev/cli-darwin-arm64",
  "darwin-x64": "@actionbookdev/cli-darwin-x64",
  "linux-x64": "@actionbookdev/cli-linux-x64-gnu",
  "linux-arm64": "@actionbookdev/cli-linux-arm64-gnu",
  "win32-x64": "@actionbookdev/cli-win32-x64",
  "win32-arm64": "@actionbookdev/cli-win32-arm64",
};

function getBinaryPath() {
  const platformKey = `${process.platform}-${process.arch}`;
  const packageName = PLATFORM_PACKAGES[platformKey];

  if (!packageName) {
    return null;
  }

  const binaryName = process.platform === "win32" ? "actionbook.exe" : "actionbook";

  const packageDir = resolvePackageDir(packageName);
  if (!packageDir) {
    return null;
  }

  return path.join(packageDir, "bin", binaryName);
}

function resolvePackageDir(packageName) {
  try {
    const packageJsonPath = require.resolve(`${packageName}/package.json`);
    return path.dirname(packageJsonPath);
  } catch {
    const unscoped = packageName.split("/")[1];
    const packageDir = path.join(__dirname, "..", "..", unscoped);
    const packageJsonPath = path.join(packageDir, "package.json");
    if (fs.existsSync(packageJsonPath)) {
      return packageDir;
    }
    return null;
  }
}

/**
 * Check if setup has already been completed by looking for the config file.
 */
function isSetupComplete() {
  const configPath = path.join(os.homedir(), ".actionbook", "config.toml");
  return fs.existsSync(configPath);
}

/**
 * Detect if running in a non-interactive (agent/CI) environment.
 */
function isNonInteractive() {
  return (
    !process.stdout.isTTY ||
    process.env.CI === "true" ||
    process.env.CI === "1" ||
    process.env.NON_INTERACTIVE === "true" ||
    process.env.NON_INTERACTIVE === "1"
  );
}

/**
 * Print setup instructions after installation.
 *
 * In non-interactive environments (agents, CI), outputs a JSON hint
 * so the calling agent can programmatically discover the setup command.
 * In interactive terminals, prints a human-friendly message.
 */
function printSetupHint(setupComplete) {
  if (setupComplete) {
    return;
  }

  if (isNonInteractive()) {
    // Machine-readable JSON for agents
    const hint = {
      actionbook_setup_required: true,
      setup_check_command: "actionbook setup --check --json",
      setup_command: "actionbook setup --non-interactive --json",
      setup_with_api_key: "actionbook setup --non-interactive --json --api-key <YOUR_KEY>",
      setup_with_target: "actionbook setup --non-interactive --json --target claude",
      available_targets: ["claude", "codex", "cursor", "windsurf", "antigravity", "opencode"],
      docs: "https://actionbook.dev/docs/setup",
    };
    console.log(JSON.stringify(hint));
  } else {
    console.log("");
    console.log("  Actionbook installed successfully!");
    console.log("");
    console.log("  Run setup to configure:");
    console.log("    $ actionbook setup");
    console.log("");
    console.log("  For non-interactive setup (CI/agents):");
    console.log("    $ actionbook setup --non-interactive --api-key <YOUR_KEY>");
    console.log("");
  }
}

function main() {
  const binaryPath = getBinaryPath();
  if (!binaryPath) {
    process.exit(0);
  }

  if (fs.existsSync(binaryPath) && process.platform !== "win32") {
    fs.chmodSync(binaryPath, 0o755);
  }

  const setupComplete = isSetupComplete();
  printSetupHint(setupComplete);
}

if (require.main === module) {
  main();
}

module.exports = { main, getBinaryPath, resolvePackageDir, PLATFORM_PACKAGES, isSetupComplete, isNonInteractive, printSetupHint };

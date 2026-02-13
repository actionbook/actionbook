#!/usr/bin/env node

/**
 * Postinstall script for @actionbookdev/cli.
 *
 * Ensures the installed platform binary keeps executable permissions.
 */

"use strict";

const fs = require("fs");
const path = require("path");

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

function main() {
  const binaryPath = getBinaryPath();
  if (!binaryPath) {
    process.exit(0);
  }

  if (fs.existsSync(binaryPath) && process.platform !== "win32") {
    fs.chmodSync(binaryPath, 0o755);
  }
}

if (require.main === module) {
  main();
}

module.exports = { main, getBinaryPath, resolvePackageDir, PLATFORM_PACKAGES };

#!/usr/bin/env node

/**
 * Cross-platform CLI wrapper for actionbook.
 *
 * Resolves the platform-specific binary package and spawns the native binary.
 * Supports ACTIONBOOK_BINARY_PATH env var for development.
 */

"use strict";

const { spawn } = require("child_process");
const { existsSync, readFileSync } = require("fs");
const path = require("path");

const PLATFORM_PACKAGES = {
  "darwin-arm64": "@actionbookdev/cli-darwin-arm64",
  "darwin-x64": "@actionbookdev/cli-darwin-x64",
  "linux-x64": "@actionbookdev/cli-linux-x64-gnu",
  "linux-arm64": "@actionbookdev/cli-linux-arm64-gnu",
  "win32-x64": "@actionbookdev/cli-win32-x64",
  "win32-arm64": "@actionbookdev/cli-win32-arm64",
};

function main() {
  // Keep CLI version aligned with npm package version.
  if (isVersionOnlyFlag(process.argv.slice(2))) {
    const pkgPath = path.join(__dirname, "..", "package.json");
    const pkg = JSON.parse(readFileSync(pkgPath, "utf8"));
    console.log(`actionbook ${pkg.version}`);
    process.exit(0);
  }

  // Allow env var override for development.
  const envPath = process.env.ACTIONBOOK_BINARY_PATH;
  if (envPath) {
    run(envPath);
    return;
  }

  const platformKey = `${process.platform}-${process.arch}`;
  const binaryPath = getBinaryPath(platformKey);
  run(binaryPath);
}

function isVersionOnlyFlag(args) {
  return args.length === 1 && (args[0] === "--version" || args[0] === "-V");
}

function getBinaryPath(platformKey) {
  const packageName = PLATFORM_PACKAGES[platformKey];

  if (!packageName) {
    console.error(`Error: Unsupported platform: ${platformKey}`);
    console.error(`Supported: ${Object.keys(PLATFORM_PACKAGES).join(", ")}`);
    process.exit(1);
  }

  if (process.platform === "linux" && isLikelyMusl()) {
    console.error(`Error: Unsupported libc for ${platformKey}`);
    console.error("This release currently supports Linux glibc only.");
    console.error("musl (for example Alpine Linux) is not supported yet.");
    process.exit(1);
  }

  const binaryName = process.platform === "win32" ? "actionbook.exe" : "actionbook";

  try {
    const packageDir = resolvePackageDir(packageName);
    if (!packageDir) {
      throw new Error("package not found");
    }
    const binaryPath = path.join(packageDir, "bin", binaryName);

    if (!existsSync(binaryPath)) {
      console.error(`Error: No binary found in ${packageName}`);
      console.error(`Expected: ${binaryPath}`);
      process.exit(1);
    }

    return binaryPath;
  } catch {
    console.error(`Error: Missing native package for ${platformKey}`);
    console.error(`Expected package: ${packageName}`);
    console.error("");
    console.error("This usually happens when optional dependencies are skipped.");
    console.error("Check if you installed with --omit=optional.");
    console.error("");
    console.error("Try reinstalling:");
    console.error("  npm install -g @actionbookdev/cli");
    console.error("");
    console.error("Or install the Rust CLI directly:");
    console.error("  cargo install actionbook");
    process.exit(1);
  }
}

function resolvePackageDir(packageName) {
  try {
    const packageJsonPath = require.resolve(`${packageName}/package.json`);
    return path.dirname(packageJsonPath);
  } catch {
    // Fallback for workspace or non-hoisted layouts.
    const unscoped = packageName.split("/")[1];
    const packageDir = path.join(__dirname, "..", "..", unscoped);
    const packageJsonPath = path.join(packageDir, "package.json");
    if (existsSync(packageJsonPath)) {
      return packageDir;
    }
    return null;
  }
}

function isLikelyMusl() {
  if (process.platform !== "linux") {
    return false;
  }

  if (!process.report || typeof process.report.getReport !== "function") {
    return false;
  }

  try {
    const report = process.report.getReport();
    const header = report && report.header ? report.header : {};
    return !header.glibcVersionRuntime;
  } catch {
    return false;
  }
}

function run(binaryPath) {
  const child = spawn(binaryPath, process.argv.slice(2), {
    stdio: "inherit",
    windowsHide: false,
  });

  child.on("error", (err) => {
    console.error(`Error executing binary: ${err.message}`);
    process.exit(1);
  });

  child.on("close", (code) => {
    process.exit(code ?? 0);
  });
}

if (require.main === module) {
  main();
}

module.exports = {
  main,
  isVersionOnlyFlag,
  getBinaryPath,
  resolvePackageDir,
  isLikelyMusl,
  run,
  PLATFORM_PACKAGES,
};

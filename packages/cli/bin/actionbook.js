#!/usr/bin/env node

/**
 * Cross-platform CLI wrapper for actionbook
 *
 * Detects the current platform and spawns the corresponding native Rust binary
 * from the same directory. Supports ACTIONBOOK_BINARY_PATH env var for development.
 */

"use strict";

const { spawn } = require("child_process");
const { existsSync } = require("fs");
const path = require("path");

const PLATFORMS = {
  "darwin-arm64": "actionbook-darwin-arm64",
  "darwin-x64": "actionbook-darwin-x64",
  "linux-x64": "actionbook-linux-x64",
  "linux-arm64": "actionbook-linux-arm64",
  "win32-x64": "actionbook-win32-x64.exe",
  "win32-arm64": "actionbook-win32-arm64.exe",
};

function main() {
  // Allow env var override for development
  const envPath = process.env.ACTIONBOOK_BINARY_PATH;
  if (envPath) {
    run(envPath);
    return;
  }

  const platformKey = `${process.platform}-${process.arch}`;
  const binaryName = PLATFORMS[platformKey];

  if (!binaryName) {
    console.error(`Error: Unsupported platform: ${platformKey}`);
    console.error(`Supported: ${Object.keys(PLATFORMS).join(", ")}`);
    process.exit(1);
  }

  const binaryPath = path.join(__dirname, binaryName);

  if (!existsSync(binaryPath)) {
    console.error(`Error: No binary found for ${platformKey}`);
    console.error(`Expected: ${binaryPath}`);
    console.error("");
    console.error("Try reinstalling:");
    console.error("  npm install -g @actionbookdev/cli");
    console.error("");
    console.error("Or install the Rust CLI directly:");
    console.error("  cargo install actionbook");
    process.exit(1);
  }

  run(binaryPath);
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

main();

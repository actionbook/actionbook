#!/usr/bin/env node

/**
 * Postinstall script for @actionbookdev/cli
 *
 * Ensures the platform binary has executable permissions.
 * (npm doesn't always preserve the execute bit)
 */

"use strict";

const fs = require("fs");
const path = require("path");

const binDir = path.join(__dirname, "..", "bin");

function getBinaryName() {
  const platformKey = `${process.platform}-${process.arch}`;
  const map = {
    "darwin-arm64": "actionbook-darwin-arm64",
    "darwin-x64": "actionbook-darwin-x64",
    "linux-x64": "actionbook-linux-x64",
    "linux-arm64": "actionbook-linux-arm64",
    "win32-x64": "actionbook-win32-x64.exe",
    "win32-arm64": "actionbook-win32-arm64.exe",
  };
  return map[platformKey] || null;
}

const binaryName = getBinaryName();
if (!binaryName) process.exit(0);

const binaryPath = path.join(binDir, binaryName);

if (fs.existsSync(binaryPath) && process.platform !== "win32") {
  fs.chmodSync(binaryPath, 0o755);
}

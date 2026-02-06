#!/usr/bin/env node

/**
 * Postinstall script for @actionbookdev/cli
 *
 * If the platform binary is missing (e.g., npm didn't include it),
 * downloads it from GitHub Releases as a fallback.
 */

"use strict";

const fs = require("fs");
const path = require("path");
const https = require("https");

const binDir = path.join(__dirname, "..", "bin");

const GITHUB_REPO = "actionbook/actionbook";

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

function downloadFile(url, dest) {
  return new Promise((resolve, reject) => {
    const request = (url) => {
      https
        .get(url, (response) => {
          if (response.statusCode === 301 || response.statusCode === 302) {
            request(response.headers.location);
            return;
          }
          if (response.statusCode !== 200) {
            reject(new Error(`HTTP ${response.statusCode}`));
            return;
          }
          const file = fs.createWriteStream(dest);
          response.pipe(file);
          file.on("finish", () => {
            file.close();
            resolve();
          });
          file.on("error", (err) => {
            fs.unlinkSync(dest);
            reject(err);
          });
        })
        .on("error", reject);
    };
    request(url);
  });
}

async function main() {
  const binaryName = getBinaryName();

  if (!binaryName) {
    console.log(
      `⚠ Unsupported platform: ${process.platform}-${process.arch}. ` +
        "Install the Rust CLI directly: cargo install actionbook"
    );
    return;
  }

  const binaryPath = path.join(binDir, binaryName);

  // Binary already exists (shipped with npm package) — just fix permissions
  if (fs.existsSync(binaryPath)) {
    if (process.platform !== "win32") {
      fs.chmodSync(binaryPath, 0o755);
    }
    return;
  }

  // Fallback: download from GitHub Releases
  const packageJson = JSON.parse(
    fs.readFileSync(path.join(__dirname, "..", "package.json"), "utf8")
  );
  const version = packageJson.version;
  const downloadUrl = `https://github.com/${GITHUB_REPO}/releases/download/actionbook-cli-v${version}/${binaryName}`;

  console.log(`Downloading actionbook binary for ${process.platform}-${process.arch}...`);

  try {
    if (!fs.existsSync(binDir)) {
      fs.mkdirSync(binDir, { recursive: true });
    }

    await downloadFile(downloadUrl, binaryPath);

    if (process.platform !== "win32") {
      fs.chmodSync(binaryPath, 0o755);
    }

    console.log(`✓ Downloaded: ${binaryName}`);
  } catch (err) {
    console.log(`⚠ Could not download binary: ${err.message}`);
    console.log("");
    console.log("To install manually:");
    console.log("  cargo install actionbook");
    console.log("  # or set ACTIONBOOK_BINARY_PATH env var");
  }
}

main().catch(console.error);

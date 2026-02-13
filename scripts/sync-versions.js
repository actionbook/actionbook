#!/usr/bin/env node

/**
 * Post-changeset version sync script.
 *
 * Runs after `changeset version` to keep derived versions in sync:
 *   1. CLI version → 6 platform package.json (version + optionalDependencies)
 *   2. Extension package.json version → manifest.json version
 */

const fs = require("fs");
const path = require("path");

const ROOT = path.resolve(__dirname, "..");
const read = (rel) =>
  JSON.parse(fs.readFileSync(path.join(ROOT, rel), "utf8"));
const write = (rel, obj) =>
  fs.writeFileSync(path.join(ROOT, rel), JSON.stringify(obj, null, 2) + "\n");

// ---------------------------------------------------------------------------
// 1. Sync CLI version → platform packages
// ---------------------------------------------------------------------------

const PLATFORM_PACKAGES = [
  "packages/cli-darwin-arm64/package.json",
  "packages/cli-darwin-x64/package.json",
  "packages/cli-linux-x64-gnu/package.json",
  "packages/cli-linux-arm64-gnu/package.json",
  "packages/cli-win32-x64/package.json",
  "packages/cli-win32-arm64/package.json",
];

const cliPkg = read("packages/cli/package.json");
const cliVersion = cliPkg.version;

// Sync platform package versions
for (const rel of PLATFORM_PACKAGES) {
  const pkg = read(rel);
  const prev = pkg.version;
  pkg.version = cliVersion;
  write(rel, pkg);
  if (prev !== cliVersion) {
    console.log(`  ${rel}: ${prev} → ${cliVersion}`);
  }
}

// Sync CLI optionalDependencies to match its own version
let cliChanged = false;
if (cliPkg.optionalDependencies) {
  for (const [name, ver] of Object.entries(cliPkg.optionalDependencies)) {
    if (name.startsWith("@actionbookdev/cli-") && ver !== cliVersion) {
      cliPkg.optionalDependencies[name] = cliVersion;
      cliChanged = true;
    }
  }
}
if (cliChanged) {
  write("packages/cli/package.json", cliPkg);
  console.log(`  packages/cli/package.json: synced optionalDependencies → ${cliVersion}`);
}

console.log(`CLI sync done (v${cliVersion})`);

// ---------------------------------------------------------------------------
// 2. Sync extension package.json version → manifest.json
// ---------------------------------------------------------------------------

const extPkgPath = "packages/actionbook-extension/package.json";
const manifestPath = "packages/actionbook-extension/manifest.json";

const extPkg = read(extPkgPath);
const manifest = read(manifestPath);
const extVersion = extPkg.version;

if (manifest.version !== extVersion) {
  const prev = manifest.version;
  manifest.version = extVersion;
  write(manifestPath, manifest);
  console.log(`  ${manifestPath}: ${prev} → ${extVersion}`);
}

console.log(`Extension sync done (v${extVersion})`);

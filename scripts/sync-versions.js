#!/usr/bin/env node

/**
 * Post-changeset version sync script.
 *
 * Runs after `changeset version` to keep derived versions in sync:
 *   1. CLI version → 6 platform package.json version
 *   2. Extension package.json version → manifest.json version
 *
 * Note: CLI optionalDependencies use workspace:* protocol so pnpm resolves
 * them locally. At publish time, the publish-cli job replaces workspace:*
 * with the actual version before npm publish.
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

// ---------------------------------------------------------------------------
// 3. Sync dify-plugin package.json version → manifest.yaml + pyproject.toml
// ---------------------------------------------------------------------------

const difyPkgPath = path.join(ROOT, "packages/dify-plugin/package.json");
const difyManifestPath = path.join(ROOT, "packages/dify-plugin/manifest.yaml");
const difyPyprojectPath = path.join(ROOT, "packages/dify-plugin/pyproject.toml");

const difyPkg = JSON.parse(fs.readFileSync(difyPkgPath, "utf8"));
const difyVersion = difyPkg.version;

// Sync manifest.yaml (two version fields: root version + meta.version)
let manifestYaml = fs.readFileSync(difyManifestPath, "utf8");
let difyChanged = false;

const rootVersionMatch = manifestYaml.match(/^version:\s*(.+)$/m);
if (!rootVersionMatch) {
  throw new Error("manifest.yaml: root 'version:' field not found");
}
if (rootVersionMatch[1].trim() !== difyVersion) {
  manifestYaml = manifestYaml.replace(
    /^(version:\s*).+$/m,
    `$1${difyVersion}`
  );
  difyChanged = true;
}

const metaVersionMatch = manifestYaml.match(/(meta:\s*\n\s+version:\s*)(.+)/);
if (!metaVersionMatch) {
  throw new Error("manifest.yaml: 'meta.version' field not found");
}
if (metaVersionMatch[2].trim() !== difyVersion) {
  manifestYaml = manifestYaml.replace(
    /(meta:\s*\n\s+version:\s*).+/,
    `$1${difyVersion}`
  );
  difyChanged = true;
}

if (difyChanged) {
  fs.writeFileSync(difyManifestPath, manifestYaml);
  console.log(`  packages/dify-plugin/manifest.yaml: → ${difyVersion}`);
}

// Sync pyproject.toml version
let pyproject = fs.readFileSync(difyPyprojectPath, "utf8");
const pyVersionMatch = pyproject.match(/^version\s*=\s*"(.+)"/m);
if (!pyVersionMatch) {
  throw new Error("pyproject.toml: 'version' field not found");
}
if (pyVersionMatch[1] !== difyVersion) {
  pyproject = pyproject.replace(
    /^(version\s*=\s*").+(")/m,
    `$1${difyVersion}$2`
  );
  fs.writeFileSync(difyPyprojectPath, pyproject);
  console.log(`  packages/dify-plugin/pyproject.toml: → ${difyVersion}`);
}

// Regenerate uv.lock so the lockfile version stays in sync with pyproject.toml
const { execSync } = require("child_process");
const difyPluginDir = path.join(ROOT, "packages/dify-plugin");
try {
  execSync("uv lock", { cwd: difyPluginDir, stdio: "inherit" });
  console.log("  uv.lock regenerated");
} catch (e) {
  console.error("  WARNING: failed to run 'uv lock':", e.message);
}

console.log(`Dify plugin sync done (v${difyVersion})`);

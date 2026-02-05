#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
NPM_DIR="$PROJECT_DIR/npm"

DRY_RUN="${DRY_RUN:-}"
NPM_TAG="${NPM_TAG:-latest}"

NPM_ARGS=("--access" "public" "--tag" "$NPM_TAG")
if [ -n "$DRY_RUN" ]; then
  NPM_ARGS+=("--dry-run")
  echo "DRY RUN mode enabled"
fi

PLATFORMS=(
  "darwin-arm64"
  "darwin-x64"
  "linux-x64"
  "linux-arm64"
  "win32-x64"
  "win32-arm64"
)

# Update versions first
bash "$SCRIPT_DIR/build-npm.sh" --version

# Publish platform packages first
echo "Publishing platform packages..."
for suffix in "${PLATFORMS[@]}"; do
  pkg_dir="$NPM_DIR/@actionbookdev/actionbook-rs-${suffix}"

  # Check if binary exists
  if [[ "$suffix" == win32-* ]]; then
    bin_file="$pkg_dir/bin/actionbook.exe"
  else
    bin_file="$pkg_dir/bin/actionbook"
  fi

  if [ ! -f "$bin_file" ] || [ "$bin_file" -nt "$bin_file" ] && [ "$(stat -f%z "$bin_file" 2>/dev/null || stat -c%s "$bin_file" 2>/dev/null)" = "0" ]; then
    # Skip .gitkeep or missing binaries
    if [ -f "$pkg_dir/bin/.gitkeep" ] && [ ! -f "$bin_file" ]; then
      echo "Skipping $suffix (no binary found)"
      continue
    fi
  fi

  echo "Publishing @actionbookdev/actionbook-rs-${suffix}..."
  (cd "$pkg_dir" && npm publish "${NPM_ARGS[@]}")
done

# Publish main wrapper package
echo "Publishing @actionbookdev/actionbook-rs..."
(cd "$NPM_DIR/actionbook-rs" && npm publish "${NPM_ARGS[@]}")

echo "Done!"

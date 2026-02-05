#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
NPM_DIR="$PROJECT_DIR/npm"

# Read version from Cargo.toml
VERSION=$(grep '^version' "$PROJECT_DIR/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/')

echo "Building actionbook v${VERSION} for npm distribution"

# Platform mapping: npm-suffix -> rust-target
declare -A TARGETS=(
  ["darwin-arm64"]="aarch64-apple-darwin"
  ["darwin-x64"]="x86_64-apple-darwin"
  ["linux-x64"]="x86_64-unknown-linux-gnu"
  ["linux-arm64"]="aarch64-unknown-linux-gnu"
  ["win32-x64"]="x86_64-pc-windows-msvc"
  ["win32-arm64"]="aarch64-pc-windows-msvc"
)

# Update version in all package.json files
update_versions() {
  echo "Updating all package.json versions to ${VERSION}..."

  # Update main wrapper package
  local main_pkg="$NPM_DIR/actionbook-rs/package.json"
  if [ -f "$main_pkg" ]; then
    # Use node for reliable JSON manipulation
    node -e "
      const fs = require('fs');
      const pkg = JSON.parse(fs.readFileSync('$main_pkg', 'utf8'));
      pkg.version = '$VERSION';
      if (pkg.optionalDependencies) {
        for (const key of Object.keys(pkg.optionalDependencies)) {
          pkg.optionalDependencies[key] = '$VERSION';
        }
      }
      fs.writeFileSync('$main_pkg', JSON.stringify(pkg, null, 2) + '\n');
    "
  fi

  # Update platform packages
  for suffix in "${!TARGETS[@]}"; do
    local pkg_file="$NPM_DIR/@actionbookdev/actionbook-rs-${suffix}/package.json"
    if [ -f "$pkg_file" ]; then
      node -e "
        const fs = require('fs');
        const pkg = JSON.parse(fs.readFileSync('$pkg_file', 'utf8'));
        pkg.version = '$VERSION';
        fs.writeFileSync('$pkg_file', JSON.stringify(pkg, null, 2) + '\n');
      "
    fi
  done

  echo "All versions updated to ${VERSION}"
}

# Build for a specific target
build_target() {
  local suffix="$1"
  local target="${TARGETS[$suffix]}"

  if [ -z "$target" ]; then
    echo "Error: Unknown platform suffix '$suffix'"
    echo "Available: ${!TARGETS[*]}"
    exit 1
  fi

  echo "Building for ${target}..."

  # Determine binary name
  local bin_name="actionbook"
  if [[ "$suffix" == win32-* ]]; then
    bin_name="actionbook.exe"
  fi

  # Build
  if command -v cross &>/dev/null && [[ "$suffix" == linux-* ]]; then
    cross build --release --target "$target" --manifest-path "$PROJECT_DIR/Cargo.toml"
  else
    cargo build --release --target "$target" --manifest-path "$PROJECT_DIR/Cargo.toml"
  fi

  # Copy binary to platform package
  local src="$PROJECT_DIR/target/${target}/release/${bin_name}"
  local dest="$NPM_DIR/@actionbookdev/actionbook-rs-${suffix}/bin/${bin_name}"

  if [ ! -f "$src" ]; then
    echo "Error: Binary not found at $src"
    exit 1
  fi

  cp "$src" "$dest"
  chmod +x "$dest"
  echo "Copied binary to $dest"
}

# Copy a pre-built binary into the correct platform package
copy_binary() {
  local suffix="$1"
  local binary_path="$2"

  local bin_name="actionbook"
  if [[ "$suffix" == win32-* ]]; then
    bin_name="actionbook.exe"
  fi

  local dest="$NPM_DIR/@actionbookdev/actionbook-rs-${suffix}/bin/${bin_name}"
  cp "$binary_path" "$dest"
  chmod +x "$dest"
  echo "Copied binary to $dest"
}

# Parse arguments
case "${1:-}" in
  --version)
    update_versions
    ;;
  --build)
    SUFFIX="${2:-}"
    if [ -z "$SUFFIX" ]; then
      echo "Usage: $0 --build <platform-suffix>"
      echo "Platforms: ${!TARGETS[*]}"
      exit 1
    fi
    build_target "$SUFFIX"
    ;;
  --copy)
    SUFFIX="${2:-}"
    BINARY="${3:-}"
    if [ -z "$SUFFIX" ] || [ -z "$BINARY" ]; then
      echo "Usage: $0 --copy <platform-suffix> <binary-path>"
      exit 1
    fi
    copy_binary "$SUFFIX" "$BINARY"
    ;;
  --build-all)
    for suffix in "${!TARGETS[@]}"; do
      build_target "$suffix" || echo "Warning: Failed to build for $suffix"
    done
    ;;
  *)
    echo "Usage: $0 <command>"
    echo ""
    echo "Commands:"
    echo "  --version              Update all package.json versions from Cargo.toml"
    echo "  --build <suffix>       Build for a specific platform"
    echo "  --build-all            Build for all platforms"
    echo "  --copy <suffix> <bin>  Copy a pre-built binary into a platform package"
    echo ""
    echo "Platforms: ${!TARGETS[*]}"
    ;;
esac

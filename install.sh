#!/usr/bin/env bash
# install.sh — Install the Actionbook CLI binary on macOS or Linux.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/actionbook/actionbook/main/install.sh | bash
#   curl -fsSL ... | bash -s -- --version 0.8.1
#   curl -fsSL ... | bash -s -- --bin-dir ~/.local/bin
#
# Options:
#   --version <VERSION>   Install a specific version (e.g. 0.8.1 or v0.8.1).
#                         Defaults to the latest release.
#   --bin-dir <DIR>       Directory to install the binary into.
#                         Defaults to /usr/local/bin (may need sudo).
#   --help                Show this help message.

set -euo pipefail

REPO="actionbook/actionbook"
BINARY_NAME="actionbook"
TMPDIR_CLEANUP=""
trap 'rm -rf "$TMPDIR_CLEANUP"' EXIT

# ── Defaults ──────────────────────────────────────────────────────────────────
VERSION=""
BIN_DIR="/usr/local/bin"

# ── Parse arguments ───────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      VERSION="$2"
      shift 2
      ;;
    --bin-dir)
      BIN_DIR="$2"
      shift 2
      ;;
    --help)
      echo "Actionbook CLI Installer"
      echo ""
      echo "Usage:"
      echo "  curl -fsSL https://raw.githubusercontent.com/actionbook/actionbook/main/install.sh | bash"
      echo "  curl -fsSL ... | bash -s -- --version 0.8.1"
      echo "  curl -fsSL ... | bash -s -- --bin-dir ~/.local/bin"
      echo ""
      echo "Options:"
      echo "  --version <VERSION>   Install a specific version (default: latest)"
      echo "  --bin-dir <DIR>       Install directory (default: /usr/local/bin)"
      echo "  --help                Show this help message"
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      exit 1
      ;;
  esac
done

# ── Helpers ───────────────────────────────────────────────────────────────────
info()  { echo "  [info]  $*"; }
error() { echo "  [error] $*" >&2; exit 1; }

need_cmd() {
  if ! command -v "$1" &>/dev/null; then
    error "Required command not found: $1"
  fi
}

# ── Prerequisite checks ──────────────────────────────────────────────────────
need_cmd curl
if ! command -v sha256sum &>/dev/null && ! command -v shasum &>/dev/null; then
  error "Required command not found: sha256sum or shasum"
fi

# ── Detect OS and ARCH ────────────────────────────────────────────────────────
detect_platform() {
  local os arch

  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Darwin) os="darwin" ;;
    Linux)  os="linux"  ;;
    *)      error "Unsupported OS: $os. Only macOS (Darwin) and Linux are supported." ;;
  esac

  case "$arch" in
    x86_64|amd64)  arch="x64"   ;;
    arm64|aarch64) arch="arm64" ;;
    *)             error "Unsupported architecture: $arch. Only x64 and arm64 are supported." ;;
  esac

  PLATFORM="${os}-${arch}"
}

# ── Resolve version ──────────────────────────────────────────────────────────
resolve_version() {
  if [[ -n "$VERSION" ]]; then
    # Strip leading 'v' if present
    VERSION="${VERSION#v}"
  else
    info "Fetching latest release version..."
    # Use GitHub API to find the most recent release matching actionbook-cli-v*.
    # The /releases endpoint returns 30 items per page; the latest CLI release
    # may not be on the first page, so we paginate up to 5 pages.
    local latest_tag="" page=1
    while [[ -z "$latest_tag" && $page -le 5 ]]; do
      latest_tag="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases?per_page=100&page=${page}" \
        | grep -o '"tag_name": *"actionbook-cli-v[^"]*"' \
        | head -1 \
        | sed 's/.*"actionbook-cli-v\([^"]*\)"/\1/')"
      page=$((page + 1))
    done

    if [[ -z "$latest_tag" ]]; then
      error "Could not determine latest release version."
    fi
    VERSION="$latest_tag"
  fi

  TAG="actionbook-cli-v${VERSION}"
  info "Version: ${VERSION} (tag: ${TAG})"
}

# ── Compute SHA-256 ──────────────────────────────────────────────────────────
sha256() {
  if command -v sha256sum &>/dev/null; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

# ── Download & verify ─────────────────────────────────────────────────────────
download_and_verify() {
  local asset_name="actionbook-${PLATFORM}"
  local download_url="https://github.com/${REPO}/releases/download/${TAG}/${asset_name}"
  local checksums_url="https://github.com/${REPO}/releases/download/${TAG}/SHA256SUMS"

  local tmpdir
  tmpdir="$(mktemp -d)"
  TMPDIR_CLEANUP="$tmpdir"

  info "Downloading ${asset_name}..."
  curl -fsSL -o "${tmpdir}/${asset_name}" "$download_url" \
    || error "Failed to download binary from ${download_url}"

  info "Downloading SHA256SUMS..."
  curl -fsSL -o "${tmpdir}/SHA256SUMS" "$checksums_url" \
    || error "Failed to download checksums from ${checksums_url}"

  info "Verifying checksum..."
  local expected actual
  expected="$(grep "  ${asset_name}$" "${tmpdir}/SHA256SUMS" | awk '{print $1}')"
  if [[ -z "$expected" ]]; then
    error "No checksum entry found for ${asset_name} in SHA256SUMS."
  fi

  actual="$(sha256 "${tmpdir}/${asset_name}")"
  if [[ "$expected" != "$actual" ]]; then
    error "Checksum mismatch!\n  Expected: ${expected}\n  Actual:   ${actual}"
  fi
  info "Checksum verified."

  # Install
  if [[ -d "$BIN_DIR" && -w "$BIN_DIR" ]]; then
    mv "${tmpdir}/${asset_name}" "${BIN_DIR}/${BINARY_NAME}"
    chmod +x "${BIN_DIR}/${BINARY_NAME}"
  elif [[ -d "$BIN_DIR" ]]; then
    info "Elevated permissions required to write to ${BIN_DIR}."
    sudo mv "${tmpdir}/${asset_name}" "${BIN_DIR}/${BINARY_NAME}"
    sudo chmod +x "${BIN_DIR}/${BINARY_NAME}"
  else
    # Directory doesn't exist yet — try to create it
    if mkdir -p "$BIN_DIR" 2>/dev/null; then
      mv "${tmpdir}/${asset_name}" "${BIN_DIR}/${BINARY_NAME}"
      chmod +x "${BIN_DIR}/${BINARY_NAME}"
    else
      info "Elevated permissions required to create ${BIN_DIR}."
      sudo mkdir -p "$BIN_DIR"
      sudo mv "${tmpdir}/${asset_name}" "${BIN_DIR}/${BINARY_NAME}"
      sudo chmod +x "${BIN_DIR}/${BINARY_NAME}"
    fi
  fi

  info "Installed ${BINARY_NAME} to ${BIN_DIR}/${BINARY_NAME}"
}

# ── PATH guidance ─────────────────────────────────────────────────────────────
check_path() {
  if ! echo "$PATH" | tr ':' '\n' | grep -qx "$BIN_DIR"; then
    echo ""
    echo "  ⚠  ${BIN_DIR} is not in your PATH."
    echo "     Add it by appending one of the following to your shell profile:"
    echo ""
    echo "       export PATH=\"${BIN_DIR}:\$PATH\""
    echo ""
    echo "     Then restart your terminal or run:  source ~/.bashrc  (or ~/.zshrc)"
  fi
}

# ── Main ──────────────────────────────────────────────────────────────────────
main() {
  echo ""
  echo "  Actionbook CLI Installer"
  echo "  ========================"
  echo ""

  detect_platform
  info "Platform: ${PLATFORM}"

  resolve_version
  download_and_verify
  check_path

  echo ""
  info "Done! Run 'actionbook --help' to get started."
  echo ""
}

main

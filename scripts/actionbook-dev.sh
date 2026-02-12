#!/bin/bash
# actionbook-dev.sh - Development workflow script for actionbook-rs
#
# Usage:
#   ./scripts/actionbook-dev.sh [command]
#
# Commands:
#   install   - Build and install actionbook-rs to ~/.cargo/bin
#   test      - Run tests
#   build     - Build release binary
#   clean     - Clean build artifacts
#   watch     - Watch for changes and rebuild

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ACTIONBOOK_RS_DIR="$PROJECT_ROOT/packages/actionbook-rs"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

info() {
    echo -e "${GREEN}ℹ${NC} $1"
}

warn() {
    echo -e "${YELLOW}⚠${NC} $1"
}

error() {
    echo -e "${RED}✗${NC} $1"
}

success() {
    echo -e "${GREEN}✓${NC} $1"
}

# Check if we're in the right directory
if [ ! -d "$ACTIONBOOK_RS_DIR" ]; then
    error "actionbook-rs directory not found at $ACTIONBOOK_RS_DIR"
    exit 1
fi

cd "$ACTIONBOOK_RS_DIR"

cmd_install() {
    info "Building and installing actionbook-rs..."
    cargo install --path . --force
    success "actionbook installed to ~/.cargo/bin/actionbook"

    # Verify installation
    info "Verifying installation..."
    actionbook --version
}

cmd_test() {
    info "Running tests..."
    cargo test
    success "Tests completed"
}

cmd_build() {
    info "Building release binary..."
    cargo build --release
    success "Release binary built at target/release/actionbook"
}

cmd_clean() {
    info "Cleaning build artifacts..."
    cargo clean
    success "Build artifacts cleaned"
}

cmd_watch() {
    info "Watching for changes and rebuilding..."
    if ! command -v cargo-watch &> /dev/null; then
        warn "cargo-watch not found. Installing..."
        cargo install cargo-watch
    fi

    cargo watch -x 'install --path . --force'
}

cmd_help() {
    cat << EOF
actionbook-dev.sh - Development workflow script for actionbook-rs

Usage:
    ./scripts/actionbook-dev.sh [command]

Commands:
    install   - Build and install actionbook-rs to ~/.cargo/bin (default)
    test      - Run tests
    build     - Build release binary
    clean     - Clean build artifacts
    watch     - Watch for changes and rebuild
    help      - Show this help message

Examples:
    # Install to global cargo bin (recommended before testing)
    ./scripts/actionbook-dev.sh install

    # Run tests
    ./scripts/actionbook-dev.sh test

    # Watch and auto-rebuild on changes
    ./scripts/actionbook-dev.sh watch

Note: Always run 'install' before testing CLI commands to ensure
      you're using the latest local build instead of an old global version.
EOF
}

# Main command dispatcher
case "${1:-install}" in
    install)
        cmd_install
        ;;
    test)
        cmd_test
        ;;
    build)
        cmd_build
        ;;
    clean)
        cmd_clean
        ;;
    watch)
        cmd_watch
        ;;
    help|--help|-h)
        cmd_help
        ;;
    *)
        error "Unknown command: $1"
        echo ""
        cmd_help
        exit 1
        ;;
esac

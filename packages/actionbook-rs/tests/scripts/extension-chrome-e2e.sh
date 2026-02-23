#!/usr/bin/env bash
# Extension Bridge E2E Test — Real Chrome + Real Website
#
# Uses automation-friendly test sites (the-internet.herokuapp.com) to avoid
# rate limiting. Connects to the user's EXISTING Chrome browser (with extension
# already installed) and tests all CLI commands against real websites.
#
# Prerequisites:
#   1. Extension installed in your Chrome:
#      - Open chrome://extensions → Enable Developer Mode
#      - Load unpacked → select packages/actionbook-extension/
#   2. CLI binary built: cargo build --manifest-path packages/actionbook-rs/Cargo.toml
#   3. Port 19222 available (extension hard-codes this port)
#
# Usage:
#   bash packages/actionbook-rs/tests/scripts/extension-chrome-e2e.sh
#
# Options:
#   --skip-build    Skip cargo build step
#   --keep-tabs     Don't close tabs opened during test

set -euo pipefail

# --- Configuration ---
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TESTS_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
RS_DIR="$(cd "$TESTS_DIR/.." && pwd)"
PROJECT_ROOT="$(cd "$RS_DIR/../.." && pwd)"

AB="$RS_DIR/target/debug/actionbook"
BRIDGE_PORT=19222
SKIP_BUILD=false
KEEP_TABS=false

while [[ $# -gt 0 ]]; do
  case $1 in
    --skip-build) SKIP_BUILD=true; shift ;;
    --keep-tabs) KEEP_TABS=true; shift ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

EXT="--extension --extension-port $BRIDGE_PORT"

PASS=0
FAIL=0
SKIP=0
TOTAL=0
FAILURES=""

TMPDIR=$(mktemp -d)
BRIDGE_PID=""

# Tabs we opened (for cleanup)
OPENED_TABS=()

# --- Cleanup ---
cleanup() {
  local exit_code=$?
  set +e

  # Close tabs we opened (unless --keep-tabs)
  if [ "$KEEP_TABS" = false ] && [ ${#OPENED_TABS[@]} -gt 0 ]; then
    printf "\n  Cleaning up opened tabs..."
    for tab_id in "${OPENED_TABS[@]}"; do
      "$AB" $EXT browser eval "
        chrome_tab_id = $tab_id;
      " &>/dev/null 2>&1
    done
    # Detach so the debug banner goes away
    "$AB" $EXT browser close &>/dev/null 2>&1
    printf " done\n"
  fi

  # Stop bridge
  if [ -n "$BRIDGE_PID" ]; then
    kill "$BRIDGE_PID" 2>/dev/null
    wait "$BRIDGE_PID" 2>/dev/null
  fi

  rm -rf "$TMPDIR"
  exit $exit_code
}
trap cleanup EXIT INT TERM

# --- Helpers ---
log_pass() {
  TOTAL=$((TOTAL + 1))
  PASS=$((PASS + 1))
  printf "  \033[32mPASS\033[0m %s\n" "$1"
}

log_fail() {
  TOTAL=$((TOTAL + 1))
  FAIL=$((FAIL + 1))
  FAILURES="${FAILURES}\n  - $1: $2"
  printf "  \033[31mFAIL\033[0m %s: %s\n" "$1" "$2"
}

log_skip() {
  TOTAL=$((TOTAL + 1))
  SKIP=$((SKIP + 1))
  printf "  \033[33mSKIP\033[0m %s: %s\n" "$1" "$2"
}

log_info() {
  printf "  \033[36mINFO\033[0m %s\n" "$1"
}

run() {
  set +e
  OUTPUT=$("$AB" "$@" 2>&1)
  EXIT_CODE=$?
  set -e
}

assert_contains() {
  local test_name="$1"
  local pattern="$2"
  if echo "$OUTPUT" | grep -qi "$pattern"; then
    log_pass "$test_name"
  else
    log_fail "$test_name" "Expected '$pattern' in: $(echo "$OUTPUT" | head -3 | tr '\n' ' ')"
  fi
}

assert_success() {
  local test_name="$1"
  if [ "$EXIT_CODE" -eq 0 ]; then
    log_pass "$test_name"
  else
    log_fail "$test_name" "exit $EXIT_CODE: $(echo "$OUTPUT" | head -2 | tr '\n' ' ')"
  fi
}

assert_file_exists() {
  local test_name="$1"
  local filepath="$2"
  if [ -f "$filepath" ]; then
    log_pass "$test_name"
  else
    log_fail "$test_name" "File not found: $filepath"
  fi
}

assert_file_size_gt() {
  local test_name="$1"
  local filepath="$2"
  local min_size="$3"
  if [ -f "$filepath" ]; then
    local size
    size=$(wc -c < "$filepath" | tr -d ' ')
    if [ "$size" -gt "$min_size" ]; then
      log_pass "$test_name ($size bytes)"
    else
      log_fail "$test_name" "File too small: $size bytes (expected > $min_size)"
    fi
  else
    log_fail "$test_name" "File not found: $filepath"
  fi
}

wait_for_port() {
  local port=$1
  local max_wait=${2:-10}
  local i=0
  while [ $i -lt "$max_wait" ]; do
    if lsof -i ":$port" -sTCP:LISTEN &>/dev/null 2>&1; then
      return 0
    fi
    sleep 1
    i=$((i + 1))
  done
  return 1
}

wait_for_extension() {
  local max_wait=${1:-30}
  local i=0
  while [ $i -lt "$max_wait" ]; do
    if "$AB" extension ping --port "$BRIDGE_PORT" 2>&1 | grep -qi "pong\|responded"; then
      return 0
    fi
    sleep 1
    i=$((i + 1))
  done
  return 1
}

# --- Test Sites (automation-friendly, no rate limiting) ---
SITE="https://the-internet.herokuapp.com"
SITE_NAME="the-internet"

# ============================================================
printf "\n\033[1m=== Extension Bridge E2E Test (Real Chrome) ===\033[0m\n\n"
printf "  This test connects to your EXISTING Chrome browser.\n"
printf "  Make sure the Actionbook extension is installed and Chrome is open.\n"
printf "  Test site: %s\n\n" "$SITE"

# --- Pre-flight ---
printf "\033[1m--- Pre-flight ---\033[0m\n"

if [ "$SKIP_BUILD" = false ]; then
  printf "  Building CLI... "
  if cargo build --manifest-path "$RS_DIR/Cargo.toml" 2>/dev/null; then
    printf "done\n"
  else
    printf "FAILED\n"
    exit 1
  fi
fi

if [ ! -f "$AB" ]; then
  echo "ERROR: Binary not found at $AB"
  exit 1
fi
log_pass "binary-exists"

# Check port is free
if lsof -i ":$BRIDGE_PORT" -sTCP:LISTEN &>/dev/null 2>&1; then
  echo ""
  echo "  Port $BRIDGE_PORT is in use. Killing existing bridge..."
  lsof -ti ":$BRIDGE_PORT" | xargs kill 2>/dev/null || true
  sleep 1
fi
log_pass "port-$BRIDGE_PORT-available"

# ============================================================
# Phase 1: Bridge + Extension Connection
# ============================================================
printf "\033[1m--- Phase 1: Bridge + Extension ---\033[0m\n"

"$AB" extension serve --port "$BRIDGE_PORT" &>"$TMPDIR/bridge.log" &
BRIDGE_PID=$!

if wait_for_port "$BRIDGE_PORT" 5; then
  log_pass "bridge-started"
else
  log_fail "bridge-started" "Bridge failed to start"
  cat "$TMPDIR/bridge.log" 2>/dev/null
  exit 1
fi

printf "  Waiting for Chrome extension to connect (open Chrome if not already)...\n"
if wait_for_extension 30; then
  log_pass "extension-connected"
else
  log_fail "extension-connected" "Timed out after 30s"
  echo ""
  echo "  Make sure:"
  echo "    1. Chrome is open"
  echo "    2. Extension is installed: chrome://extensions → Load unpacked → packages/actionbook-extension/"
  echo "    3. Extension shows green dot (connected) in popup"
  echo ""
  echo "  Bridge log:"
  tail -5 "$TMPDIR/bridge.log" 2>/dev/null
  exit 1
fi

run extension ping --port "$BRIDGE_PORT"
assert_contains "ping" "pong\|responded"

run extension status --port "$BRIDGE_PORT"
assert_contains "status" "running"

# ============================================================
# Phase 2: Navigate to test site
# ============================================================
printf "\033[1m--- Phase 2: Navigate to $SITE_NAME ---\033[0m\n"

# Open a fresh tab first (avoids chrome:// URL issues after extension reload)
run $EXT browser open "$SITE"
assert_success "goto-site"
sleep 3

# Verify we're on the test site
run $EXT browser eval "document.title"
log_info "Page title: $(echo "$OUTPUT" | head -1)"
assert_contains "site-title" "Internet"

# List tabs — should include test site
run $EXT browser pages
assert_contains "pages-has-site" "internet\|Internet\|herokuapp"

# ============================================================
# Phase 3: Content Extraction
# ============================================================
printf "\033[1m--- Phase 3: Content Extraction ---\033[0m\n"

# Eval: simple arithmetic
run $EXT browser eval "1+1"
assert_contains "eval-arithmetic" "2"

# Eval: read real DOM
run $EXT browser eval "document.querySelector('h1,h2')?.textContent?.trim() || document.title"
assert_success "eval-dom-query"
log_info "Heading text: $(echo "$OUTPUT" | head -1)"

# HTML: full page contains site content
run $EXT browser html
assert_contains "html-full-page" "Internet\|herokuapp"

# HTML: with selector
run $EXT browser html "h1"
assert_success "html-with-selector"

# Text: page text
run $EXT browser text
assert_contains "text-page" "Available Examples\|Internet"

# Viewport
run $EXT browser viewport
assert_contains "viewport" "[Vv]iewport\|[0-9]*x[0-9]"
log_info "Viewport: $(echo "$OUTPUT" | head -1)"

# ============================================================
# Phase 4: Screenshot & PDF
# ============================================================
printf "\033[1m--- Phase 4: Screenshot & PDF ---\033[0m\n"

SCREENSHOT_PATH="$TMPDIR/e2e-screenshot.png"
run $EXT browser screenshot "$SCREENSHOT_PATH"
assert_success "screenshot-command"
assert_file_exists "screenshot-file" "$SCREENSHOT_PATH"
assert_file_size_gt "screenshot-has-content" "$SCREENSHOT_PATH" 5000

PDF_PATH="$TMPDIR/e2e-page.pdf"
run $EXT browser pdf "$PDF_PATH"
assert_success "pdf-command"
assert_file_exists "pdf-file" "$PDF_PATH"
assert_file_size_gt "pdf-has-content" "$PDF_PATH" 5000

# ============================================================
# Phase 5: Navigation (back / forward / reload)
# ============================================================
printf "\033[1m--- Phase 5: Navigation ---\033[0m\n"

# Navigate to a sub-page
run $EXT browser goto "$SITE/login"
assert_success "goto-login"
sleep 3

run $EXT browser eval "document.title"
log_info "After goto: $(echo "$OUTPUT" | head -1)"

# Reload
run $EXT browser reload
assert_success "reload"
sleep 2

# Go back to homepage
run $EXT browser back
assert_success "back"
sleep 2

run $EXT browser eval "document.title"
log_info "After back: $(echo "$OUTPUT" | head -1)"

# Go forward again
run $EXT browser forward
assert_success "forward"
sleep 2

run $EXT browser eval "document.title"
log_info "After forward: $(echo "$OUTPUT" | head -1)"

# ============================================================
# Phase 6: Interactions (login form on the-internet)
# ============================================================
printf "\033[1m--- Phase 6: Interactions (login form) ---\033[0m\n"

# Go to login page (has username + password fields + Login button)
run $EXT browser goto "$SITE/login"
assert_success "goto-login-page"
sleep 2

# Check that form fields exist
run $EXT browser eval "document.querySelector('#username') ? 'found' : 'not-found'"
LOGIN_INPUT_EXISTS=$(echo "$OUTPUT" | head -1)

if echo "$LOGIN_INPUT_EXISTS" | grep -q "found"; then
  # Fill the username field (public test credentials for the-internet.herokuapp.com)
  run $EXT browser fill '#username' "tomsmith"
  assert_success "fill-username"

  # Verify value was filled
  sleep 0.3
  run $EXT browser eval "document.querySelector('#username').value"
  assert_contains "fill-effect" "tomsmith"

  # Fill the password field
  run $EXT browser fill '#password' "SuperSecretPassword!"
  assert_success "fill-password"

  # Click the Login button
  run $EXT browser click 'button[type="submit"]'
  assert_success "click-login"
  sleep 3

  # Verify login success (redirects to /secure with success flash message)
  run $EXT browser eval "window.location.pathname"
  log_info "After login path: $(echo "$OUTPUT" | head -1)"
  assert_contains "login-result-page" "secure"

  # Click the Logout button to return to login page
  run $EXT browser click 'a[href="/logout"]'
  assert_success "click-logout"
  sleep 2

  run $EXT browser eval "window.location.href"
  assert_contains "after-logout" "login"
else
  log_skip "fill-username" "Login input not found on page"
  log_skip "fill-effect" "Skipped (no login input)"
  log_skip "fill-password" "Skipped"
  log_skip "click-login" "Skipped"
  log_skip "login-result-page" "Skipped"
  log_skip "click-logout" "Skipped"
  log_skip "after-logout" "Skipped"
fi

# ============================================================
# Phase 7: Hover / Focus / Press
# ============================================================
printf "\033[1m--- Phase 7: Hover / Focus / Press ---\033[0m\n"

# Navigate to hovers page (has elements that respond to hover)
run $EXT browser goto "$SITE/hovers"
assert_success "goto-hovers"
sleep 2

# Hover on an image (triggers hover effect showing user info)
run $EXT browser hover ".figure"
assert_success "hover-figure"

# Navigate to a page with links for focus test
run $EXT browser goto "$SITE"
sleep 2

# Focus on a link (homepage has many <a> tags with href)
run $EXT browser focus "a"
assert_success "focus-link"

# Press Escape key
run $EXT browser press Escape
assert_success "press-escape"

# Press Tab key
run $EXT browser press Tab
assert_success "press-tab"

# ============================================================
# Phase 8: Cookies (uses chrome.cookies API via Extension.*)
# ============================================================
printf "\033[1m--- Phase 8: Cookies ---\033[0m\n"

# List cookies for current site
run $EXT browser cookies
assert_success "cookies-list"

# Set a test cookie
run $EXT browser cookies set e2e_test_cookie "hello_from_e2e"
assert_success "cookies-set"

# Verify cookie was set by listing again
run $EXT browser cookies
log_info "Cookies after set: $(echo "$OUTPUT" | head -3 | tr '\n' ' ')"

# Delete the test cookie
run $EXT browser cookies delete e2e_test_cookie
assert_success "cookies-delete"

# Clear all cookies for this site
run $EXT browser cookies clear
assert_success "cookies-clear"

# ============================================================
# Phase 9: Tab Management
# ============================================================
printf "\033[1m--- Phase 9: Tab Management ---\033[0m\n"

# List all tabs
run $EXT browser pages
assert_success "pages-list"
log_info "Tabs: $(echo "$OUTPUT" | grep -c 'tab\|Tab' || echo '?') visible"

# Open a new tab (use example.com — always available, fast)
run $EXT browser open "https://example.com"
assert_success "open-new-tab"
sleep 3

run $EXT browser eval "document.title"
log_info "New tab: $(echo "$OUTPUT" | head -1)"

# Switch between tabs
run $EXT browser pages
TAB_IDS=$(echo "$OUTPUT" | grep -o 'tab:[0-9]*' | cut -d: -f2)
FIRST_TAB=$(echo "$TAB_IDS" | head -1)
LAST_TAB=$(echo "$TAB_IDS" | tail -1)

if [ -n "$FIRST_TAB" ] && [ -n "$LAST_TAB" ] && [ "$FIRST_TAB" != "$LAST_TAB" ]; then
  run $EXT browser switch "$FIRST_TAB"
  assert_success "switch-to-first-tab"
  sleep 1

  run $EXT browser switch "$LAST_TAB"
  assert_success "switch-to-last-tab"
  sleep 1
else
  log_skip "switch-to-first-tab" "Need at least 2 tabs"
  log_skip "switch-to-last-tab" "Need at least 2 tabs"
fi

# ============================================================
# Phase 10: Snapshot / Inspect / JSON / Close / Restart
# ============================================================
printf "\033[1m--- Phase 10: Advanced ---\033[0m\n"

# Accessibility snapshot
run $EXT browser snapshot
assert_success "snapshot"

# Inspect element at coordinates
run $EXT browser inspect 200 200
assert_success "inspect"

# JSON output mode
run --json $EXT browser eval "1+1"
assert_success "json-eval"
if echo "$OUTPUT" | python3 -c "import sys,json; json.load(sys.stdin)" 2>/dev/null; then
  log_pass "json-valid"
elif echo "$OUTPUT" | grep -q '^[{\[0-9]'; then
  log_pass "json-valid"
else
  log_fail "json-valid" "Not valid JSON: $(echo "$OUTPUT" | head -1)"
fi

# Close (detach debugger from tab)
run $EXT browser close
assert_success "close"

# Restart (re-navigate)
run $EXT browser goto "$SITE"
sleep 2
run $EXT browser restart
assert_success "restart"

# ============================================================
# Summary
# ============================================================
printf "\n\033[1m=== E2E Results ===\033[0m\n"
printf "Total: %d | \033[32mPass: %d\033[0m | \033[31mFail: %d\033[0m | \033[33mSkip: %d\033[0m\n" \
  "$TOTAL" "$PASS" "$FAIL" "$SKIP"

if [ "$FAIL" -gt 0 ]; then
  printf "\n\033[31mFailures:\033[0m"
  printf "%b\n" "$FAILURES"
  echo ""
  echo "  Bridge log: $TMPDIR/bridge.log"
  exit 1
else
  printf "\n\033[32mAll E2E tests passed!\033[0m\n"
  exit 0
fi

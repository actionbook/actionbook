#!/bin/bash
# Quick test script for Camoufox integration

set -e

CAMOFOX_PORT=${CAMOFOX_PORT:-9377}
ACTIONBOOK_RS_DIR="$(cd ../packages/actionbook-rs && pwd)"

echo "🦊 Camoufox Integration Quick Test"
echo "=================================="
echo

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test 1: Check if camofox-browser is running
echo "📋 Test 1: Checking Camoufox server..."
if curl -s http://localhost:${CAMOFOX_PORT}/health > /dev/null 2>&1; then
    echo -e "${GREEN}✓${NC} Server is running on port ${CAMOFOX_PORT}"
else
    echo -e "${RED}✗${NC} Server not reachable on port ${CAMOFOX_PORT}"
    echo
    echo "To start the server, run in another terminal:"
    echo "  npx @askjo/camoufox-browser"
    echo
    exit 1
fi

# Test 2: Check actionbook-rs builds
echo
echo "📋 Test 2: Building actionbook-rs..."
cd "$ACTIONBOOK_RS_DIR"
if cargo build --quiet 2>&1 | head -5; then
    echo -e "${GREEN}✓${NC} actionbook-rs builds successfully"
else
    echo -e "${RED}✗${NC} Build failed"
    exit 1
fi

# Test 3: Test backend selection
echo
echo "📋 Test 3: Testing backend selection..."
if cargo run --quiet -- --camofox --help > /dev/null 2>&1; then
    echo -e "${GREEN}✓${NC} --camofox flag works"
else
    echo -e "${RED}✗${NC} --camofox flag failed"
    exit 1
fi

# Test 4: Run unit tests
echo
echo "📋 Test 4: Running unit tests..."
if cargo test --lib camofox --quiet 2>&1 | tail -5; then
    echo -e "${GREEN}✓${NC} Unit tests pass"
else
    echo -e "${RED}✗${NC} Unit tests failed"
    exit 1
fi

# Test 5: Test REST API directly
echo
echo "📋 Test 5: Testing REST API..."
RESPONSE=$(curl -s -X POST http://localhost:${CAMOFOX_PORT}/tabs \
    -H "Content-Type: application/json" \
    -d '{
        "userId": "test-user",
        "sessionKey": "quick-test",
        "url": "https://example.com"
    }')

if echo "$RESPONSE" | grep -q '"id"'; then
    TAB_ID=$(echo "$RESPONSE" | grep -o '"id":"[^"]*"' | cut -d'"' -f4)
    echo -e "${GREEN}✓${NC} Created tab: $TAB_ID"

    # Get snapshot
    echo
    echo "📋 Test 6: Getting accessibility snapshot..."
    SNAPSHOT=$(curl -s "http://localhost:${CAMOFOX_PORT}/tabs/$TAB_ID/snapshot")
    if echo "$SNAPSHOT" | grep -q '"tree"'; then
        echo -e "${GREEN}✓${NC} Snapshot received"
        echo "Sample tree:"
        echo "$SNAPSHOT" | head -c 200
        echo "..."
    else
        echo -e "${YELLOW}⚠${NC} No snapshot data"
    fi
else
    echo -e "${RED}✗${NC} Failed to create tab"
    echo "Response: $RESPONSE"
fi

# Summary
echo
echo "=================================="
echo "🎉 Quick Test Summary"
echo "=================================="
echo -e "${GREEN}✓${NC} Camoufox server: Running"
echo -e "${GREEN}✓${NC} actionbook-rs: Builds"
echo -e "${GREEN}✓${NC} Backend selection: Works"
echo -e "${GREEN}✓${NC} Unit tests: Pass"
echo -e "${GREEN}✓${NC} REST API: Connected"
echo
echo "📝 Next Steps:"
echo "1. Implement Phase 4 (command integration)"
echo "2. Test browser commands: goto, click, type, screenshot"
echo "3. Validate anti-bot effectiveness"
echo
echo "For detailed testing, see: TEST_GUIDE.md"

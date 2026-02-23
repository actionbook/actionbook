#!/bin/bash
# Test Camoufox REST API endpoints

BASE_URL="http://localhost:9377"

echo "🧪 Testing Camoufox REST API Server"
echo "===================================="
echo

# Test 1: Health check
echo "1. Testing /health endpoint..."
curl -s "$BASE_URL/health" | jq '.'
echo

# Test 2: Create tab
echo "2. Creating tab..."
TAB_RESPONSE=$(curl -s -X POST "$BASE_URL/tabs" \
  -H "Content-Type: application/json" \
  -d '{"userId":"test","sessionKey":"test","url":"https://example.com"}')
echo "$TAB_RESPONSE" | jq '.'

TAB_ID=$(echo "$TAB_RESPONSE" | jq -r '.id')
echo "Tab ID: $TAB_ID"
echo

# Test 3: Get accessibility tree
echo "3. Getting accessibility tree..."
curl -s "$BASE_URL/tabs/$TAB_ID/snapshot?user_id=test" | jq '.tree | {role, name, children: (.children | length)}'
echo

# Test 4: Screenshot
echo "4. Taking screenshot..."
SCREENSHOT_DATA=$(curl -s "$BASE_URL/tabs/$TAB_ID/screenshot?user_id=test" | jq -r '.data')
SCREENSHOT_SIZE=$(echo "$SCREENSHOT_DATA" | wc -c)
echo "Screenshot size: $SCREENSHOT_SIZE bytes (base64)"
echo

# Save screenshot
echo "$SCREENSHOT_DATA" | base64 -d > /tmp/camoufox-test.png
echo "Screenshot saved to: /tmp/camoufox-test.png"
file /tmp/camoufox-test.png
echo

echo "✅ All API tests completed!"
echo
echo "To view screenshot:"
echo "  open /tmp/camoufox-test.png"

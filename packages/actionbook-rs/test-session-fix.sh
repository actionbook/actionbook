#!/bin/bash
# Test session management fix

BASE_URL="http://127.0.0.1:9377"
SESSION_KEY="test-session-fix"
USER_ID="test-user"

echo "=== Step 1: Create tab and navigate to bot.sannysoft.com ==="
TAB_RESPONSE=$(curl -s -X POST "$BASE_URL/tabs" \
  -H "Content-Type: application/json" \
  -d "{\"user_id\": \"$USER_ID\", \"session_key\": \"$SESSION_KEY\", \"url\": \"https://bot.sannysoft.com\"}")

echo "$TAB_RESPONSE" | jq
TAB_ID=$(echo "$TAB_RESPONSE" | jq -r '.id')
echo "Created tab: $TAB_ID"

echo ""
echo "=== Step 2: Wait for page to load ==="
sleep 25

echo ""
echo "=== Step 3: Query active tab for session ==="
ACTIVE_TAB=$(curl -s "$BASE_URL/sessions/$SESSION_KEY/active-tab")
echo "$ACTIVE_TAB" | jq
ACTIVE_TAB_ID=$(echo "$ACTIVE_TAB" | jq -r '.tab_id')

if [ "$TAB_ID" = "$ACTIVE_TAB_ID" ]; then
    echo "✅ SUCCESS: Active tab matches created tab ($TAB_ID)"
else
    echo "❌ FAIL: Active tab mismatch (created: $TAB_ID, active: $ACTIVE_TAB_ID)"
    exit 1
fi

echo ""
echo "=== Step 4: Take screenshot of active tab ==="
curl -s "$BASE_URL/tabs/$ACTIVE_TAB_ID/screenshot?user_id=$USER_ID" | \
  jq -r '.data' | \
  base64 -d > /tmp/sannysoft-fix-test.png

FILE_SIZE=$(ls -lh /tmp/sannysoft-fix-test.png | awk '{print $5}')
echo "Screenshot saved: /tmp/sannysoft-fix-test.png ($FILE_SIZE)"

if [ $(stat -f%z /tmp/sannysoft-fix-test.png) -gt 100000 ]; then
    echo "✅ SUCCESS: Screenshot is valid (>100KB)"
    open /tmp/sannysoft-fix-test.png
else
    echo "❌ FAIL: Screenshot too small (likely blank page)"
    exit 1
fi

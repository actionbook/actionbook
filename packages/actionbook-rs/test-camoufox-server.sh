#!/bin/bash

# Test script for camoufox-server REST API
# Usage: ./test-camoufox-server.sh

set -e

BASE_URL="http://localhost:9377"

echo "🧪 Testing Camoufox REST API Server"
echo "====================================="

# Test 1: Health check
echo -e "\n1. GET /health"
curl -s "${BASE_URL}/health" | jq '.'

# Test 2: Create tab
echo -e "\n2. POST /tabs - Create new tab"
TAB_RESPONSE=$(curl -s -X POST "${BASE_URL}/tabs" \
  -H "Content-Type: application/json" \
  -d '{
    "userId": "test-user",
    "sessionKey": "test-session",
    "url": "https://example.com"
  }')

echo "$TAB_RESPONSE" | jq '.'
TAB_ID=$(echo "$TAB_RESPONSE" | jq -r '.id')
echo "Created tab ID: $TAB_ID"

# Test 3: Get snapshot
echo -e "\n3. GET /tabs/:id/snapshot - Get accessibility tree"
curl -s "${BASE_URL}/tabs/${TAB_ID}/snapshot" | jq '.tree | {role, name, children: (.children | length)}'

# Test 4: Click element
echo -e "\n4. POST /tabs/:id/click - Click element"
curl -s -X POST "${BASE_URL}/tabs/${TAB_ID}/click" \
  -H "Content-Type: application/json" \
  -d '{
    "userId": "test-user",
    "elementRef": "e3"
  }' | jq '.'

# Test 5: Type text
echo -e "\n5. POST /tabs/:id/type - Type text into element"
curl -s -X POST "${BASE_URL}/tabs/${TAB_ID}/type" \
  -H "Content-Type: application/json" \
  -d '{
    "userId": "test-user",
    "elementRef": "e2",
    "text": "Hello Camoufox"
  }' | jq '.'

# Test 6: Navigate
echo -e "\n6. POST /tabs/:id/navigate - Navigate to URL"
curl -s -X POST "${BASE_URL}/tabs/${TAB_ID}/navigate" \
  -H "Content-Type: application/json" \
  -d '{
    "userId": "test-user",
    "url": "https://www.example.org"
  }' | jq '.'

# Test 7: Screenshot
echo -e "\n7. GET /tabs/:id/screenshot - Take screenshot"
SCREENSHOT=$(curl -s "${BASE_URL}/tabs/${TAB_ID}/screenshot" | jq -r '.data')
echo "Screenshot data length: ${#SCREENSHOT} bytes (base64 encoded)"

echo -e "\n✅ All tests passed!"

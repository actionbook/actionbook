#!/bin/bash
# =============================================================================
# Etsy Valentine's Day Exploration Example
# =============================================================================
# This example demonstrates how to use actionbook-rs CLI to:
# 1. Search the Actionbook database for website actions
# 2. Retrieve detailed action information with selectors
# 3. Automate browser operations on Etsy.com
# =============================================================================

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${BLUE}=== Etsy Valentine's Day Exploration ===${NC}\n"

# -----------------------------------------------------------------------------
# Step 1: Search for Etsy-related actions in Actionbook
# -----------------------------------------------------------------------------
echo -e "${YELLOW}Step 1: Searching for Etsy actions in Actionbook...${NC}"

actionbook search "etsy" --page-size 5

echo -e "\n${GREEN}Found Etsy actions in the database.${NC}\n"

# -----------------------------------------------------------------------------
# Step 2: Search for Valentine's Day related content
# -----------------------------------------------------------------------------
echo -e "${YELLOW}Step 2: Searching for Valentine's Day content...${NC}"

actionbook search "valentine" --domain etsy.com --page-size 5

echo -e "\n${GREEN}Found Valentine's Day related actions.${NC}\n"

# -----------------------------------------------------------------------------
# Step 3: Get detailed action by area ID
# -----------------------------------------------------------------------------
echo -e "${YELLOW}Step 3: Getting detailed action by area ID...${NC}"

# Area ID format: site:path:area
# This returns element selectors, XPath, and interaction methods
actionbook get "etsy.com:/:default"

echo -e "\n${GREEN}Retrieved detailed action with selectors.${NC}\n"

# -----------------------------------------------------------------------------
# Step 4: Check browser status
# -----------------------------------------------------------------------------
echo -e "${YELLOW}Step 4: Checking browser status...${NC}"

actionbook browser status

echo ""

# -----------------------------------------------------------------------------
# Step 5: Open Etsy Valentine's Day page
# -----------------------------------------------------------------------------
echo -e "${YELLOW}Step 5: Opening Etsy Valentine's Day gifts page...${NC}"

actionbook browser open "https://www.etsy.com/market/valentines_day_gifts"

echo -e "${GREEN}Browser opened.${NC}\n"

# Wait for page to load
sleep 2

# -----------------------------------------------------------------------------
# Step 6: Take a screenshot
# -----------------------------------------------------------------------------
echo -e "${YELLOW}Step 6: Taking screenshot...${NC}"

SCREENSHOT_PATH="/tmp/etsy_valentine_$(date +%Y%m%d_%H%M%S).png"
actionbook browser screenshot "$SCREENSHOT_PATH"

echo -e "${GREEN}Screenshot saved to: $SCREENSHOT_PATH${NC}\n"

# -----------------------------------------------------------------------------
# Step 7: Navigate to Best of Valentine's Day
# -----------------------------------------------------------------------------
echo -e "${YELLOW}Step 7: Navigating to Best of Valentine's Day...${NC}"

actionbook browser goto "https://www.etsy.com/featured/best-of-valentines-day"

echo -e "${GREEN}Navigated successfully.${NC}\n"

# Wait for page to load
sleep 2

# -----------------------------------------------------------------------------
# Step 8: Take another screenshot
# -----------------------------------------------------------------------------
echo -e "${YELLOW}Step 8: Taking final screenshot...${NC}"

SCREENSHOT_PATH2="/tmp/etsy_best_valentine_$(date +%Y%m%d_%H%M%S).png"
actionbook browser screenshot "$SCREENSHOT_PATH2"

echo -e "${GREEN}Screenshot saved to: $SCREENSHOT_PATH2${NC}\n"

# -----------------------------------------------------------------------------
# Step 9: Get page title via JavaScript
# -----------------------------------------------------------------------------
echo -e "${YELLOW}Step 9: Getting page title...${NC}"

actionbook browser eval "document.title"

echo ""

# -----------------------------------------------------------------------------
# Step 10: List available sources
# -----------------------------------------------------------------------------
echo -e "${YELLOW}Step 10: Listing available sources...${NC}"

actionbook sources list --limit 5

echo ""

# -----------------------------------------------------------------------------
# Summary
# -----------------------------------------------------------------------------
echo -e "${BLUE}=== Exploration Complete ===${NC}"
echo -e "Screenshots saved:"
echo -e "  - $SCREENSHOT_PATH"
echo -e "  - $SCREENSHOT_PATH2"
echo ""
echo -e "Key Actionbook CLI commands used:"
echo -e "  ${GREEN}actionbook search${NC}     - Search for website actions"
echo -e "  ${GREEN}actionbook get${NC}        - Get action details with selectors"
echo -e "  ${GREEN}actionbook browser${NC}    - Browser automation commands"
echo -e "  ${GREEN}actionbook sources${NC}    - List/search available sources"

#!/usr/bin/env python3
"""Test script to verify Camoufox fingerprint protection on bot.sannysoft.com"""

import requests
import time
import base64
import json

BASE_URL = "http://127.0.0.1:9377"

def main():
    # 1. Health check
    resp = requests.get(f"{BASE_URL}/health")
    print(f"✓ Server health: {resp.json()}")

    # 2. Create tab and navigate to bot.sannysoft.com
    print("\n📍 Creating tab for https://bot.sannysoft.com...")
    resp = requests.post(
        f"{BASE_URL}/tabs",
        json={
            "user_id": "test-user",
            "session_key": "test-session",
            "url": "https://bot.sannysoft.com"
        }
    )
    tab_data = resp.json()
    tab_id = tab_data["id"]
    print(f"✓ Tab created: {tab_id}")

    # 3. Wait for page to load
    print("\n⏳ Waiting 30 seconds for page to fully load...")
    time.sleep(30)

    # 4. Take screenshot
    print(f"\n📸 Taking screenshot of tab {tab_id}...")
    resp = requests.get(
        f"{BASE_URL}/tabs/{tab_id}/screenshot",
        params={"user_id": "test-user"}
    )

    resp_data = resp.json()
    print(f"Response keys: {resp_data.keys()}")
    screenshot_b64 = resp_data.get("screenshot") or resp_data.get("data")
    screenshot_bytes = base64.b64decode(screenshot_b64)

    # 5. Save screenshot
    output_path = "/tmp/sannysoft-correct.png"
    with open(output_path, "wb") as f:
        f.write(screenshot_bytes)

    print(f"✓ Screenshot saved: {output_path} ({len(screenshot_bytes)} bytes)")
    print(f"\n🎉 Test complete! Open the screenshot to verify detection results.")
    print(f"   Expected: Most checks should be GREEN (Camoufox fingerprint protection)")

if __name__ == "__main__":
    main()

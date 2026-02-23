#!/usr/bin/env python3
"""Test Camoufox anti-bot capabilities on Reddit."""

import asyncio
from browser import CamofoxBrowserManager


async def test_reddit():
    """Test navigating to Reddit without CAPTCHA."""
    print("🧪 Testing Camoufox Anti-Bot on Reddit...")
    print("=" * 60)

    manager = CamofoxBrowserManager()

    # NOTE: Browser will be headful (headless=False in browser.py)

    try:
        # Navigate to Reddit
        print("\n1. Navigating to reddit.com/r/rust...")
        tab = await manager.create_tab("test-user", "test-session", "https://www.reddit.com/r/rust")
        print(f"✅ Tab created: {tab.id}")
        print(f"   URL: {tab.url}")

        # Wait for page to fully load
        print("\n2. Waiting 5 seconds for page to load...")
        await asyncio.sleep(5)

        # Take screenshot
        print("\n3. Taking screenshot...")
        screenshot_b64 = await manager.screenshot(tab.id, "test-user")

        # Save screenshot
        import base64
        screenshot_data = base64.b64decode(screenshot_b64)
        screenshot_path = "/tmp/reddit-test.png"
        with open(screenshot_path, "wb") as f:
            f.write(screenshot_data)
        print(f"✅ Screenshot saved: {screenshot_path}")
        print(f"   Size: {len(screenshot_data)} bytes")

        # Get page title from accessibility tree
        print("\n4. Getting page info...")
        tree = await manager.get_accessibility_tree(tab.id, "test-user")
        print(f"✅ Page title: {tree.get('name', 'N/A')}")

        print("\n" + "=" * 60)
        print("🎉 Test Complete!")
        print("")
        print("Next steps:")
        print(f"1. Open screenshot: open {screenshot_path}")
        print("2. Check if CAPTCHA appears")
        print("3. If no CAPTCHA → Camoufox stealth works! ✅")
        print("4. If CAPTCHA appears → Need to adjust config ⚠️")
        print("")
        print("Browser window should be visible (headless=False)")
        print("Press Ctrl+C to close browser and exit")
        print("=" * 60)

        # Keep browser open for manual inspection
        await asyncio.sleep(300)  # 5 minutes

    except KeyboardInterrupt:
        print("\n\n🛑 Test interrupted by user")

    except Exception as e:
        print(f"\n❌ Test failed: {e}")
        import traceback
        traceback.print_exc()

    finally:
        # Cleanup
        print("\n🧹 Cleaning up...")
        await manager.cleanup()
        print("✅ Cleanup complete")


if __name__ == "__main__":
    asyncio.run(test_reddit())

"""Quick test script to verify server startup."""

import asyncio
from browser import CamofoxBrowserManager


async def test_browser():
    """Test browser initialization and basic operations."""
    print("🧪 Testing Camoufox Browser Manager...")

    manager = CamofoxBrowserManager()

    try:
        # Test tab creation
        print("\n1. Creating tab...")
        tab = await manager.create_tab("test-user", "test-session", "https://example.com")
        print(f"✅ Tab created: {tab.id}")
        print(f"   URL: {tab.url}")

        # Test accessibility tree
        print("\n2. Getting accessibility tree...")
        tree = await manager.get_accessibility_tree(tab.id, "test-user")
        print(f"✅ Accessibility tree retrieved")
        print(f"   Root role: {tree['role']}")
        print(f"   Root name: {tree.get('name')}")

        # Test screenshot
        print("\n3. Taking screenshot...")
        screenshot_b64 = await manager.screenshot(tab.id, "test-user")
        print(f"✅ Screenshot taken: {len(screenshot_b64)} chars (base64)")

        print("\n✅ All tests passed!")

    except Exception as e:
        print(f"\n❌ Test failed: {e}")
        import traceback
        traceback.print_exc()

    finally:
        # Cleanup
        print("\n4. Cleaning up...")
        await manager.cleanup()
        print("✅ Cleanup complete")


if __name__ == "__main__":
    asyncio.run(test_browser())

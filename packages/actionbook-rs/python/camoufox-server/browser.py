"""Camoufox browser manager with tab management."""

from camoufox import AsyncCamoufox
from playwright.async_api import Page, Browser
import asyncio
from typing import Dict, Optional
import uuid
import base64
from fastapi import HTTPException


class TabState:
    """State for a single browser tab."""

    def __init__(self, id: str, page: Page, url: str):
        self.id = id
        self.page = page
        self.url = url
        self.element_map: Dict[str, str] = {}  # element_ref -> selector mapping
        self.ref_counter = 0


class CamofoxBrowserManager:
    """Manages Camoufox browser instance and tabs."""

    def __init__(self):
        self.browser: Optional[Browser] = None
        self.camoufox_ctx = None
        self.tabs: Dict[str, TabState] = {}
        self.session_tabs: Dict[str, str] = {}  # session_key -> latest_tab_id mapping
        self._lock = asyncio.Lock()

    async def _ensure_browser(self):
        """Lazy browser initialization with enhanced anti-detection."""
        if self.browser is None:
            # Create AsyncCamoufox context manager with humanization enabled
            self.camoufox_ctx = AsyncCamoufox(
                headless=False,  # Show browser window for debugging

                # CRITICAL: Enable humanization for behavior simulation
                humanize=True,  # Enable human-like mouse movements

                # Use default fingerprint protection - Camoufox auto-generates
                # BrowserForge will automatically generate realistic fingerprints
            )

            # Enter context to get browser
            self.browser = await self.camoufox_ctx.__aenter__()
            print("✅ Camoufox browser launched with HUMANIZATION ENABLED")
            print("   - Human-like mouse movements: ACTIVE")
            print("   - BrowserForge auto-generated fingerprints: ACTIVE")
            print("   - C++-level anti-detection: ACTIVE")

    async def create_tab(self, user_id: str, session_key: str, url: str) -> 'TabState':
        """Create a new tab and navigate to URL."""
        async with self._lock:
            await self._ensure_browser()

            tab_id = str(uuid.uuid4())
            page = await self.browser.new_page()

            # Navigate to URL
            try:
                await page.goto(url, wait_until="domcontentloaded", timeout=30000)
            except Exception as e:
                await page.close()
                raise HTTPException(status_code=500, detail=f"Navigation failed: {str(e)}")

            tab = TabState(id=tab_id, page=page, url=url)
            self.tabs[tab_id] = tab
            self.session_tabs[session_key] = tab_id  # Remember this tab for the session

            print(f"✅ Tab created: {tab_id} -> {url}")
            return tab

    def _get_tab(self, tab_id: str, user_id: str) -> TabState:
        """Get tab by ID, raise 404 if not found."""
        tab = self.tabs.get(tab_id)
        if not tab:
            available_tabs = list(self.tabs.keys())
            raise HTTPException(
                status_code=404,
                detail=f"Tab not found: {tab_id}. Available tabs: {available_tabs}"
            )
        return tab

    def get_active_tab_for_session(self, session_key: str) -> Optional[str]:
        """Get the active tab ID for a given session key."""
        return self.session_tabs.get(session_key)

    async def get_accessibility_tree(self, tab_id: str, user_id: str):
        """Extract accessibility tree from page."""
        tab = self._get_tab(tab_id, user_id)

        try:
            # Get accessibility snapshot from Playwright
            snapshot = await tab.page.accessibility.snapshot()
            if snapshot is None:
                raise HTTPException(status_code=500, detail="Failed to get accessibility snapshot")

            # Convert to our format with element refs
            tree = self._convert_snapshot(snapshot, tab)
            return tree

        except Exception as e:
            raise HTTPException(status_code=500, detail=f"Failed to get accessibility tree: {str(e)}")

    def _convert_snapshot(self, node, tab: TabState):
        """Convert Playwright accessibility node to our format with element refs."""
        if node is None:
            return None

        result = {
            "role": node.get("role", "generic"),
            "name": node.get("name"),
        }

        # Generate element ref for interactive elements
        if node.get("name") and self._is_interactive(node.get("role")):
            tab.ref_counter += 1
            element_ref = f"e{tab.ref_counter}"
            result["elementRef"] = element_ref

            # Store mapping (use text selector as simple approach)
            # In production, would use more robust selector strategy
            selector = f'text="{node["name"]}"'
            tab.element_map[element_ref] = selector

        # Process children recursively
        if "children" in node and node["children"]:
            result["children"] = [
                self._convert_snapshot(child, tab)
                for child in node["children"]
                if child is not None
            ]

        return result

    def _is_interactive(self, role: Optional[str]) -> bool:
        """Check if element role is interactive or potentially clickable."""
        if role is None:
            return False

        # Expand to include more interactive/clickable roles
        interactive_roles = {
            # Form controls
            "button", "link", "textbox", "checkbox", "radio",
            "combobox", "menuitem", "tab", "switch", "searchbox",

            # Navigation
            "navigation", "menubar", "menu", "menuitemcheckbox",
            "menuitemradio", "option", "progressbar", "scrollbar",
            "slider", "spinbutton", "tablist", "tabpanel",

            # Document structure (often clickable)
            "heading", "article", "section", "banner", "complementary",
            "contentinfo", "form", "main", "region", "search",

            # Text/media (can be interacted with)
            "paragraph", "listitem", "img", "figure",
        }
        return role.lower() in interactive_roles

    async def click(self, tab_id: str, user_id: str, element_ref: str):
        """Click element by element ref."""
        tab = self._get_tab(tab_id, user_id)

        # Resolve element_ref to selector
        selector = tab.element_map.get(element_ref)
        if not selector:
            raise HTTPException(
                status_code=400,
                detail=f"Unknown element ref: {element_ref}. Available refs: {list(tab.element_map.keys())}"
            )

        try:
            await tab.page.click(selector, timeout=5000)
            print(f"✅ Clicked element: {element_ref} ({selector})")
        except Exception as e:
            raise HTTPException(status_code=500, detail=f"Click failed: {str(e)}")

    async def type_text(self, tab_id: str, user_id: str, element_ref: str, text: str):
        """Type text into element."""
        tab = self._get_tab(tab_id, user_id)

        selector = tab.element_map.get(element_ref)
        if not selector:
            raise HTTPException(
                status_code=400,
                detail=f"Unknown element ref: {element_ref}"
            )

        try:
            await tab.page.fill(selector, text, timeout=5000)
            print(f"✅ Typed into element: {element_ref} ({selector})")
        except Exception as e:
            raise HTTPException(status_code=500, detail=f"Type failed: {str(e)}")

    async def navigate(self, tab_id: str, user_id: str, url: str):
        """Navigate tab to URL."""
        tab = self._get_tab(tab_id, user_id)

        try:
            await tab.page.goto(url, wait_until="domcontentloaded", timeout=30000)
            tab.url = url
            # Clear element map on navigation
            tab.element_map.clear()
            tab.ref_counter = 0
            print(f"✅ Navigated to: {url}")
        except Exception as e:
            raise HTTPException(status_code=500, detail=f"Navigation failed: {str(e)}")

    async def screenshot(self, tab_id: str, user_id: str) -> str:
        """Take screenshot and return base64 encoded PNG."""
        tab = self._get_tab(tab_id, user_id)

        try:
            screenshot_bytes = await tab.page.screenshot(type="png")
            encoded = base64.b64encode(screenshot_bytes).decode("utf-8")
            print(f"✅ Screenshot taken: {len(screenshot_bytes)} bytes")
            return encoded
        except Exception as e:
            raise HTTPException(status_code=500, detail=f"Screenshot failed: {str(e)}")

    async def cleanup(self):
        """Cleanup all resources."""
        print("🧹 Cleaning up browser resources...")

        # Close all tabs
        for tab_id, tab in list(self.tabs.items()):
            try:
                await tab.page.close()
            except:
                pass

        self.tabs.clear()

        # Close browser via context manager
        if self.camoufox_ctx:
            try:
                await self.camoufox_ctx.__aexit__(None, None, None)
            except:
                pass
            self.camoufox_ctx = None
            self.browser = None

        print("✅ Cleanup complete")

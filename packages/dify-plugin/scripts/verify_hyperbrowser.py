#!/usr/bin/env python3
"""Quick manual verification script for Hyperbrowser connectivity.

Usage:
    HYPERBROWSER_API_KEY=hb-xxx uv run python scripts/verify_hyperbrowser.py

What it does:
    1. Creates a Hyperbrowser session
    2. Connects via CDP (Playwright)
    3. Navigates to example.com
    4. Reads page text
    5. Takes a screenshot (saved to /tmp/hb_verify.png)
    6. Stops the session

If all steps pass, your Hyperbrowser API key is working and
you can run the full E2E test suite.
"""

import os
import sys
import time


def main():
    api_key = os.environ.get("HYPERBROWSER_API_KEY")
    if not api_key:
        print("ERROR: Set HYPERBROWSER_API_KEY environment variable first.")
        print("  export HYPERBROWSER_API_KEY=hb-your-key-here")
        sys.exit(1)

    print(f"API Key: {api_key[:8]}...{api_key[-4:]}")
    print()

    # Step 1: Create session
    print("[1/6] Creating Hyperbrowser session...")
    t0 = time.time()

    from hyperbrowser import Hyperbrowser
    from hyperbrowser.models import CreateSessionParams

    client = Hyperbrowser(api_key=api_key)
    session = client.sessions.create(params=CreateSessionParams(use_proxy=False))

    print(f"      Session ID: {session.id}")
    print(f"      WS Endpoint: {session.ws_endpoint[:60]}...")
    print(f"      ({time.time() - t0:.1f}s)")
    print()

    try:
        # Step 2: Connect via CDP
        print("[2/6] Connecting via CDP (Playwright)...")
        t0 = time.time()

        from playwright.sync_api import sync_playwright

        with sync_playwright() as p:
            browser = p.chromium.connect_over_cdp(session.ws_endpoint, timeout=30000)
            contexts = browser.contexts
            if contexts:
                page = contexts[0].pages[0] if contexts[0].pages else contexts[0].new_page()
            else:
                ctx = browser.new_context()
                page = ctx.new_page()

            print(f"      Connected! ({time.time() - t0:.1f}s)")
            print()

            # Step 3: Navigate
            print("[3/6] Navigating to https://example.com ...")
            t0 = time.time()
            page.goto("https://example.com", wait_until="domcontentloaded", timeout=30000)
            print(f"      URL: {page.url}")
            print(f"      Title: {page.title()}")
            print(f"      ({time.time() - t0:.1f}s)")
            print()

            # Step 4: Read text
            print("[4/6] Reading page text...")
            body_text = page.inner_text("body")
            preview = body_text[:120].replace("\n", " ")
            print(f"      Text preview: {preview}...")
            assert "Example Domain" in body_text, "Expected 'Example Domain' in page text"
            print("      Assertion passed.")
            print()

            # Step 5: Screenshot
            print("[5/6] Taking screenshot...")
            screenshot_path = "/tmp/hb_verify.png"
            screenshot_bytes = page.screenshot(full_page=True)
            with open(screenshot_path, "wb") as f:
                f.write(screenshot_bytes)
            print(f"      Saved to: {screenshot_path} ({len(screenshot_bytes)} bytes)")
            assert screenshot_bytes[:4] == b"\x89PNG", "Not a valid PNG"
            print()

            browser.close()

    finally:
        # Step 6: Stop session
        print("[6/6] Stopping session...")
        t0 = time.time()
        client.sessions.stop(session.id)
        print(f"      Session stopped. ({time.time() - t0:.1f}s)")
        print()

    print("=" * 50)
    print("ALL CHECKS PASSED")
    print("=" * 50)
    print()
    print("Your Hyperbrowser API key is working.")
    print("Run the full E2E suite with:")
    print()
    print(f"  HYPERBROWSER_API_KEY={api_key[:8]}... uv run pytest -m e2e -v --timeout=120 --no-cov")


if __name__ == "__main__":
    main()

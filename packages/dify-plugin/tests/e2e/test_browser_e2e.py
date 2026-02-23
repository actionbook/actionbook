"""E2E browser tests — real Hyperbrowser sessions.

Tests are split into two categories:

1. **Tool wrapper tests**: Each test creates its own session, does ONE operator
   call, and stops. This validates the real Dify tool behavior accurately
   (each tool call = one CDP connect/disconnect cycle).

2. **Playwright direct tests**: Keep a single CDP connection open for multi-step
   workflows (navigate → click → fill → snapshot). This is how real browser
   automation works — the Dify workflow would need to keep the connection alive.

Run with:
    HYPERBROWSER_API_KEY=hb-xxx uv run pytest -m e2e -v --timeout=120 --no-cov
"""

import logging
import ssl
import time
from urllib.parse import urlparse

import pytest
from playwright.sync_api import sync_playwright

from tests.e2e.conftest import _is_ssl_error, parse_session_output
from tools.browser_create_session import BrowserCreateSessionTool
from tools.browser_operator import BrowserOperatorTool
from tools.browser_stop_session import BrowserStopSessionTool

pytestmark = pytest.mark.e2e

logger = logging.getLogger(__name__)

MAX_RETRIES = 3
RETRY_DELAY = 2


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _make_create_tool() -> BrowserCreateSessionTool:
    return BrowserCreateSessionTool.from_credentials({})


def _make_operator_tool() -> BrowserOperatorTool:
    return BrowserOperatorTool.from_credentials({})


def _make_stop_tool() -> BrowserStopSessionTool:
    return BrowserStopSessionTool.from_credentials({})


def _invoke_with_retry(tool, params: dict, retries: int = MAX_RETRIES) -> list:
    """Invoke a Dify tool with retry for gevent SSL errors."""
    last_exc = None
    for attempt in range(1, retries + 1):
        try:
            return list(tool._invoke(params))
        except (ssl.SSLError, OSError, ConnectionError) as exc:
            last_exc = exc
            if attempt < retries and _is_ssl_error(exc):
                logger.warning(
                    "SSL error in tool invoke (attempt %d/%d): %s — retrying",
                    attempt, retries, exc,
                )
                time.sleep(RETRY_DELAY)
            else:
                raise
        except Exception as exc:
            if _is_ssl_error(exc):
                last_exc = exc
                if attempt < retries:
                    logger.warning(
                        "SSL error in tool invoke (attempt %d/%d): %s — retrying",
                        attempt, retries, exc,
                    )
                    time.sleep(RETRY_DELAY)
                else:
                    raise
            else:
                raise
    raise last_exc  # type: ignore[misc]


def _connect_cdp_with_retry(playwright, ws_endpoint: str, retries: int = MAX_RETRIES):
    """Connect to CDP with retry for gevent SSL errors."""
    last_exc = None
    for attempt in range(1, retries + 1):
        try:
            return playwright.chromium.connect_over_cdp(ws_endpoint, timeout=30000)
        except (ssl.SSLError, OSError, ConnectionError) as exc:
            last_exc = exc
            if attempt < retries and _is_ssl_error(exc):
                logger.warning(
                    "SSL error in CDP connect (attempt %d/%d): %s — retrying",
                    attempt, retries, exc,
                )
                time.sleep(RETRY_DELAY)
            else:
                raise
        except Exception as exc:
            if _is_ssl_error(exc):
                last_exc = exc
                if attempt < retries:
                    logger.warning(
                        "SSL error in CDP connect (attempt %d/%d): %s — retrying",
                        attempt, retries, exc,
                    )
                    time.sleep(RETRY_DELAY)
                else:
                    raise
            else:
                raise
    raise last_exc  # type: ignore[misc]


# ---------------------------------------------------------------------------
# 1. Tool wrapper tests (one operation per session)
#
# Why one-op-per-session: browser_operator's cdp_page() opens a new Playwright
# connection and closes it after each _invoke(). Hyperbrowser sessions become
# unreachable (404) after the CDP client disconnects, so only the first
# connect_over_cdp succeeds.
# ---------------------------------------------------------------------------


class TestToolCreateAndStop:
    """Validate create_session and stop_session tools end-to-end."""

    def test_create_session_returns_ws_endpoint(self, hyperbrowser_api_key):
        """create_session should return valid ws_endpoint and session_id."""
        create_tool = _make_create_tool()
        stop_tool = _make_stop_tool()

        result = _invoke_with_retry(create_tool, {
            "provider": "hyperbrowser",
            "api_key": hyperbrowser_api_key,
        })

        assert len(result) == 1
        text = result[0].message.text
        assert "Error" not in text, f"Session creation failed: {text}"

        session = parse_session_output(text)
        assert session["ws_endpoint"].startswith("wss://")
        assert len(session["session_id"]) > 0
        assert session["provider"] == "hyperbrowser"

        # Clean up
        stop_result = _invoke_with_retry(stop_tool, {
            "provider": "hyperbrowser",
            "api_key": hyperbrowser_api_key,
            "session_id": session["session_id"],
        })
        assert "stopped" in stop_result[0].message.text.lower()

    def test_stop_session_works(self, hyperbrowser_api_key):
        """stop_session should succeed for an active session."""
        create_tool = _make_create_tool()
        stop_tool = _make_stop_tool()

        create_text = _invoke_with_retry(create_tool, {
            "provider": "hyperbrowser",
            "api_key": hyperbrowser_api_key,
        })[0].message.text
        session = parse_session_output(create_text)

        result = _invoke_with_retry(stop_tool, {
            "provider": "hyperbrowser",
            "api_key": hyperbrowser_api_key,
            "session_id": session["session_id"],
        })

        assert len(result) == 1
        assert "stopped" in result[0].message.text.lower()
        assert session["session_id"] in result[0].message.text


class TestToolSingleOperation:
    """Each test creates a fresh session, does ONE browser operation, then stops.

    This validates that each individual browser_operator action works through
    the real Dify tool pipeline.
    """

    def test_navigate(self, hyperbrowser_api_key):
        """navigate action works end-to-end."""
        create_tool = _make_create_tool()
        operator_tool = _make_operator_tool()
        stop_tool = _make_stop_tool()

        create_text = _invoke_with_retry(create_tool, {
            "provider": "hyperbrowser",
            "api_key": hyperbrowser_api_key,
        })[0].message.text
        session = parse_session_output(create_text)

        try:
            result = _invoke_with_retry(operator_tool, {
                "cdp_url": session["ws_endpoint"],
                "action": "navigate",
                "url": "https://example.com",
                "timeout_ms": 30000,
            })

            assert len(result) == 1
            assert "Navigation successful" in result[0].message.text
            assert "Example Domain" in result[0].message.text
        finally:
            _invoke_with_retry(stop_tool, {
                "provider": "hyperbrowser",
                "api_key": hyperbrowser_api_key,
                "session_id": session["session_id"],
            })

    def test_snapshot(self, hyperbrowser_api_key):
        """snapshot action returns accessibility tree text."""
        create_tool = _make_create_tool()
        operator_tool = _make_operator_tool()
        stop_tool = _make_stop_tool()

        create_text = _invoke_with_retry(create_tool, {
            "provider": "hyperbrowser",
            "api_key": hyperbrowser_api_key,
        })[0].message.text
        session = parse_session_output(create_text)

        try:
            result = _invoke_with_retry(operator_tool, {
                "cdp_url": session["ws_endpoint"],
                "action": "snapshot",
            })

            assert len(result) == 1
            text = result[0].message.text
            assert "Page snapshot" in text
            assert "Interactive elements" in text
        finally:
            _invoke_with_retry(stop_tool, {
                "provider": "hyperbrowser",
                "api_key": hyperbrowser_api_key,
                "session_id": session["session_id"],
            })


# ---------------------------------------------------------------------------
# 2. Playwright direct tests (multi-step with single connection)
#
# These use the hyperbrowser_session fixture which creates a session via SDK
# and provides the ws_endpoint. Playwright keeps the connection open for the
# entire test, enabling multi-step workflows.
# ---------------------------------------------------------------------------


class TestPlaywrightDirect:
    """Multi-step browser workflows using Playwright directly.

    These tests validate the full browser automation chain that a Dify workflow
    would execute when keeping a single CDP connection alive.
    """

    def test_navigate_read_text_snapshot(self, hyperbrowser_session):
        """Navigate → read text → snapshot lifecycle."""
        ws = hyperbrowser_session["ws_endpoint"]

        with sync_playwright() as p:
            browser = _connect_cdp_with_retry(p, ws)
            try:
                ctx = browser.contexts[0] if browser.contexts else browser.new_context()
                page = ctx.pages[0] if ctx.pages else ctx.new_page()

                # Navigate
                page.goto("https://example.com", wait_until="domcontentloaded", timeout=30000)
                assert page.title() == "Example Domain"

                # Read text
                body = page.inner_text("body")
                assert "Example Domain" in body

                # Snapshot
                snap = page.accessibility.snapshot()
                assert snap is not None
            finally:
                browser.close()

    def test_form_fill_workflow(self, hyperbrowser_session):
        """Navigate → wait → fill → read: simulates form interaction."""
        ws = hyperbrowser_session["ws_endpoint"]

        with sync_playwright() as p:
            browser = _connect_cdp_with_retry(p, ws)
            try:
                ctx = browser.contexts[0] if browser.contexts else browser.new_context()
                page = ctx.pages[0] if ctx.pages else ctx.new_page()

                # Navigate to form page
                page.goto(
                    "https://httpbin.org/forms/post",
                    wait_until="domcontentloaded",
                    timeout=30000,
                )

                # Wait for form
                page.wait_for_selector("input[name='custname']", timeout=10000)

                # Fill
                page.fill("input[name='custname']", "Actionbook E2E")

                # Verify fill worked
                value = page.input_value("input[name='custname']")
                assert value == "Actionbook E2E"
            finally:
                browser.close()

    def test_navigation_history(self, hyperbrowser_session):
        """Navigate two pages → history.back() → history.forward()."""
        ws = hyperbrowser_session["ws_endpoint"]

        with sync_playwright() as p:
            browser = _connect_cdp_with_retry(p, ws)
            try:
                ctx = browser.contexts[0] if browser.contexts else browser.new_context()
                page = ctx.pages[0] if ctx.pages else ctx.new_page()

                # Page 1
                page.goto("https://example.com", wait_until="domcontentloaded", timeout=30000)
                assert urlparse(page.url).hostname == "example.com"

                # Page 2
                page.goto("https://httpbin.org/html", wait_until="domcontentloaded", timeout=30000)
                assert urlparse(page.url).hostname == "httpbin.org"

                # Go back via JS (avoids Playwright's navigation wait which times
                # out on back-forward cache hits in remote browsers)
                page.evaluate("() => window.history.back()")
                page.wait_for_timeout(2000)
                assert urlparse(page.url).hostname == "example.com"

                # Go forward via JS
                page.evaluate("() => window.history.forward()")
                page.wait_for_timeout(2000)
                assert urlparse(page.url).hostname == "httpbin.org"
            finally:
                browser.close()

    def test_get_html_content(self, hyperbrowser_session):
        """Navigate and retrieve HTML content."""
        ws = hyperbrowser_session["ws_endpoint"]

        with sync_playwright() as p:
            browser = _connect_cdp_with_retry(p, ws)
            try:
                ctx = browser.contexts[0] if browser.contexts else browser.new_context()
                page = ctx.pages[0] if ctx.pages else ctx.new_page()

                page.goto("https://example.com", wait_until="domcontentloaded", timeout=30000)

                html = page.content()
                assert "<h1>Example Domain</h1>" in html
                assert "<!doctype html>" in html.lower() or "<html" in html.lower()
            finally:
                browser.close()

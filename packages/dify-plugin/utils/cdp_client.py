"""Shared CDP connection utility.

Provider-agnostic: takes a raw WebSocket URL (cdp_url) and yields an active
Playwright Page. The caller is responsible for obtaining the URL (e.g., from
browser_create_session tool output).

Usage:
    from utils.cdp_client import cdp_page, CdpConnectionError

    with cdp_page("wss://connect.steel.dev?apiKey=...") as page:
        page.goto("https://example.com")
        text = page.inner_text("body")
"""

import logging
from contextlib import contextmanager
from typing import Generator

from playwright.sync_api import Browser, Page, sync_playwright

logger = logging.getLogger(__name__)


class CdpConnectionError(Exception):
    """Raised when a CDP connection cannot be established or used."""


@contextmanager
def cdp_page(cdp_url: str, timeout_ms: int = 30000) -> Generator[Page, None, None]:
    """
    Context manager: connect to a remote browser and yield an active Page.

    Creates a new Playwright connection per call (stateless from Playwright's
    perspective). The remote browser maintains its own session state (cookies,
    localStorage) between calls.

    Args:
        cdp_url:    WebSocket or HTTP CDP endpoint.
                    Examples:
                      "wss://production-sfo.browserless.io?token=TOKEN"
                      "wss://connect.steel.dev?apiKey=KEY&sessionId=UUID"
                      "ws://localhost:9222"
        timeout_ms: Connection timeout in milliseconds (default 30 000).

    Yields:
        An active Playwright Page object.

    Raises:
        CdpConnectionError: Connection failed or an unexpected error occurred.
    """
    cdp_url = validate_cdp_url(cdp_url)

    with sync_playwright() as p:
        try:
            browser: Browser = p.chromium.connect_over_cdp(
                cdp_url,
                timeout=float(timeout_ms),
            )
        except Exception as e:
            raise CdpConnectionError(
                f"Cannot connect to browser at '{cdp_url}': {e}. "
                "Verify the CDP URL is correct and the browser session is active."
            ) from e

        try:
            page = _get_active_page(browser)
            yield page
        finally:
            # Disconnect Playwright client; does NOT close the remote browser.
            browser.close()


def _get_active_page(browser: Browser) -> Page:
    """Return an existing page or create one if the context is empty.

    Tab selection: uses the first browser context and its first page.
    If the remote browser has multiple contexts or tabs, only the first
    context's first page is returned. Create a new context + page when
    none exist.
    """
    contexts = browser.contexts
    if not contexts:
        context = browser.new_context()
        return context.new_page()

    context = contexts[0]
    pages = context.pages
    return pages[0] if pages else context.new_page()


def validate_cdp_url(cdp_url: str) -> str:
    """Validate and normalise a CDP URL string."""
    url = cdp_url.strip() if cdp_url else ""

    if not url:
        raise ValueError("'cdp_url' is required and cannot be empty")

    valid_prefixes = ("ws://", "wss://", "http://", "https://")
    if not any(url.startswith(p) for p in valid_prefixes):
        raise ValueError(
            f"Invalid cdp_url: '{url}'. "
            "Must start with ws://, wss://, http://, or https://"
        )

    # Warn when using insecure schemes with non-localhost hosts
    if url.startswith(("ws://", "http://")):
        from urllib.parse import urlparse

        parsed = urlparse(url)
        host = (parsed.hostname or "").lower()
        if host not in ("localhost", "127.0.0.1", "::1"):
            logger.warning(
                "Insecure CDP connection to non-localhost host '%s'. "
                "Consider using wss:// or https:// for remote connections.",
                host,
            )

    return url

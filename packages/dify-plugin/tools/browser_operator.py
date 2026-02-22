"""Browser Operator Tool — unified browser page operation dispatcher.

Consolidates 15 individual browser tools into one with action-based dispatch.
All action-specific parameter validation is performed by ``_pre_validate``
*before* the CDP connection is established, so handler functions assume their
required parameters are already present.
"""

import logging
from collections.abc import Callable, Generator
from typing import Any

from dify_plugin import Tool
from dify_plugin.entities.tool import ToolInvokeMessage
from playwright.sync_api import TimeoutError as PlaywrightTimeout

from utils.cdp_client import CdpConnectionError, cdp_page

logger = logging.getLogger(__name__)


def _safe_timeout(raw: Any, default: int = 30000) -> int:
    """Safely convert a raw timeout value to int, returning *default* on failure."""
    try:
        return max(0, int(raw))
    except (TypeError, ValueError):
        return default


# ---------------------------------------------------------------------------
# Action handlers
# Pre-conditions: _pre_validate has already run, so required params are present.
# Signature: (tool, page, params) -> Generator[ToolInvokeMessage, None, None]
# ---------------------------------------------------------------------------


def _handle_navigate(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Navigate to a URL."""
    url = (params.get("url") or "").strip()
    timeout_ms = _safe_timeout(params.get("timeout_ms"))

    page.goto(url, timeout=float(timeout_ms), wait_until="domcontentloaded")
    yield tool.create_text_message(
        f"Navigation successful.\nURL: {page.url}\nTitle: {page.title()}"
    )


def _handle_click(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Wait for element then click it."""
    selector = (params.get("selector") or "").strip()
    timeout_ms = _safe_timeout(params.get("timeout_ms"))

    if timeout_ms > 0:
        try:
            page.wait_for_selector(selector, timeout=float(timeout_ms))
        except PlaywrightTimeout:
            logger.debug("wait_for_selector failed for '%s'", selector, exc_info=True)
            yield tool.create_text_message(
                f"Element not found: '{selector}' within {timeout_ms}ms. "
                "Check the selector or increase timeout_ms."
            )
            return

    page.click(selector)
    yield tool.create_text_message(f"Clicked: '{selector}'")


def _handle_type(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Type text character-by-character into an input (appends)."""
    selector = (params.get("selector") or "").strip()
    text = params.get("text") or ""

    page.type(selector, text)
    yield tool.create_text_message(f"Typed into '{selector}': '{text}'")


def _handle_fill(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Clear an input then fill it with text (atomic set). Empty string clears the field."""
    selector = (params.get("selector") or "").strip()
    text = params.get("text") or ""

    page.fill(selector, text)
    yield tool.create_text_message(f"Filled '{selector}' with: '{text}'")


def _handle_select(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Select an option by value in a <select> element."""
    selector = (params.get("selector") or "").strip()
    value = (params.get("value") or "").strip()

    selected = page.select_option(selector, value=value)
    if not selected:
        yield tool.create_text_message(
            f"No option with value='{value}' found in '{selector}'."
        )
        return
    yield tool.create_text_message(f"Selected '{value}' in '{selector}'.")


def _handle_press_key(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Send a keyboard key press."""
    key = (params.get("key") or "").strip()

    page.keyboard.press(key)
    yield tool.create_text_message(f"Pressed key: '{key}'")


def _handle_hover(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Move mouse over an element to trigger hover effects."""
    selector = (params.get("selector") or "").strip()

    page.hover(selector)
    yield tool.create_text_message(f"Hovered over: '{selector}'")


def _handle_screenshot(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Capture a screenshot; yields blob then text."""
    full_page_raw = params.get("full_page", "false")
    full_page = full_page_raw is True or str(full_page_raw).lower() == "true"

    screenshot_bytes = page.screenshot(full_page=full_page)
    yield tool.create_blob_message(
        blob=screenshot_bytes,
        meta={"mime_type": "image/png"},
    )
    yield tool.create_text_message(f"Screenshot captured. URL: {page.url}")


def _handle_get_text(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Extract visible text from the page or a specific element."""
    selector = (params.get("selector") or "").strip() or None

    if selector:
        element = page.query_selector(selector)
        if element is None:
            yield tool.create_text_message(
                f"No element found for selector: '{selector}'"
            )
            return
        text = element.inner_text()
    else:
        text = page.inner_text("body")

    yield tool.create_text_message(text or "(empty)")


def _handle_get_html(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Retrieve HTML of the page or a specific element."""
    selector = (params.get("selector") or "").strip() or None

    if selector:
        element = page.query_selector(selector)
        if element is None:
            yield tool.create_text_message(
                f"No element found for selector: '{selector}'"
            )
            return
        html = element.inner_html()
    else:
        html = page.content()

    yield tool.create_text_message(html or "(empty)")


def _handle_wait(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Wait for an element to appear in the DOM."""
    selector = (params.get("selector") or "").strip()
    timeout_ms = _safe_timeout(params.get("timeout_ms"))

    try:
        page.wait_for_selector(selector, timeout=float(timeout_ms))
        yield tool.create_text_message(f"Element found: '{selector}'")
    except PlaywrightTimeout:
        logger.debug("wait_for_selector timed out for '%s'", selector, exc_info=True)
        yield tool.create_text_message(
            f"Element not found: '{selector}' within {timeout_ms}ms."
        )


def _handle_wait_navigation(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Wait for page navigation/load to complete."""
    timeout_ms = _safe_timeout(params.get("timeout_ms"))

    try:
        page.wait_for_load_state("domcontentloaded", timeout=float(timeout_ms))
        yield tool.create_text_message(
            f"Navigation complete.\nURL: {page.url}\nTitle: {page.title()}"
        )
    except Exception:
        logger.debug("wait_for_load_state timed out", exc_info=True)
        yield tool.create_text_message(
            f"Navigation did not complete within {timeout_ms}ms. "
            f"Current URL: {page.url}"
        )


def _handle_go_back(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Navigate to the previous page in history."""
    page.go_back()
    yield tool.create_text_message(f"Navigated back.\nURL: {page.url}")


def _handle_go_forward(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Navigate to the next page in history."""
    page.go_forward()
    yield tool.create_text_message(f"Navigated forward.\nURL: {page.url}")


def _handle_reload(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Reload the current page."""
    page.reload(wait_until="domcontentloaded")
    yield tool.create_text_message(f"Page reloaded.\nURL: {page.url}")


# ---------------------------------------------------------------------------
# Dispatch table
# ---------------------------------------------------------------------------

_HandlerFn = Callable[
    [Tool, Any, dict[str, Any]],
    Generator[ToolInvokeMessage, None, None],
]

_HANDLERS: dict[str, _HandlerFn] = {
    "navigate": _handle_navigate,
    "click": _handle_click,
    "type": _handle_type,
    "fill": _handle_fill,
    "select": _handle_select,
    "press_key": _handle_press_key,
    "hover": _handle_hover,
    "screenshot": _handle_screenshot,
    "get_text": _handle_get_text,
    "get_html": _handle_get_html,
    "wait": _handle_wait,
    "wait_navigation": _handle_wait_navigation,
    "go_back": _handle_go_back,
    "go_forward": _handle_go_forward,
    "reload": _handle_reload,
}


def _pre_validate(action: str, params: dict[str, Any]) -> str | None:
    """Validate action-specific required params before establishing CDP connection.

    Returns an error message string if validation fails, None if valid.
    """
    url = (params.get("url") or "").strip()
    selector = (params.get("selector") or "").strip()
    text = params.get("text") or ""
    value = (params.get("value") or "").strip()
    key = (params.get("key") or "").strip()

    if action == "navigate":
        if not url:
            return "Error: 'url' is required for action 'navigate'."
        if not url.startswith(("http://", "https://")):
            return f"Error: 'url' must start with http:// or https://. Got: '{url}'"
    elif action in ("click", "hover"):
        if not selector:
            return f"Error: 'selector' is required for action '{action}'."
    elif action == "type":
        if not selector:
            return "Error: 'selector' is required for action 'type'."
        if text == "":
            return "Error: 'text' is required for action 'type'."
    elif action == "fill":
        if not selector:
            return "Error: 'selector' is required for action 'fill'."
    elif action == "select":
        if not selector:
            return "Error: 'selector' is required for action 'select'."
        if not value:
            return "Error: 'value' is required for action 'select'."
    elif action == "press_key":
        if not key:
            return "Error: 'key' is required for action 'press_key'."
    elif action == "wait":
        if not selector:
            return "Error: 'selector' is required for action 'wait'."
    return None


class BrowserOperatorTool(Tool):
    def _invoke(self, tool_parameters: dict[str, Any]) -> Generator[ToolInvokeMessage, None, None]:
        cdp_url = (tool_parameters.get("cdp_url") or "").strip()
        action = (tool_parameters.get("action") or "").strip()

        if not cdp_url:
            yield self.create_text_message("Error: 'cdp_url' is required.")
            return
        if not action:
            yield self.create_text_message("Error: 'action' is required.")
            return
        if action not in _HANDLERS:
            valid = ", ".join(sorted(_HANDLERS))
            yield self.create_text_message(
                f"Error: Unknown action '{action}'. Valid actions: {valid}"
            )
            return

        # Validate action-specific params before establishing CDP connection
        pre_error = _pre_validate(action, tool_parameters)
        if pre_error:
            yield self.create_text_message(pre_error)
            return

        handler = _HANDLERS[action]
        try:
            with cdp_page(cdp_url) as page:
                yield from handler(self, page, tool_parameters)
        except CdpConnectionError as e:
            yield self.create_text_message(f"Error: {e}")
        except Exception as e:
            logger.exception("browser_operator action=%s failed", action)
            yield self.create_text_message(
                f"Error: Action '{action}' failed: {type(e).__name__}: {e}"
            )

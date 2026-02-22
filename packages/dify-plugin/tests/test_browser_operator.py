"""Tests for the unified BrowserOperatorTool."""

import sys
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent))

from playwright.sync_api import TimeoutError as PlaywrightTimeout

from tools.browser_operator import BrowserOperatorTool, _pre_validate
from utils.cdp_client import CdpConnectionError

VALID_CDP_URL = "ws://localhost:9222"


def _mock_cdp_page(page: MagicMock):
    """Return a patch context manager that yields the given mock page."""
    cm = MagicMock()
    cm.__enter__ = MagicMock(return_value=page)
    cm.__exit__ = MagicMock(return_value=False)
    return cm


@pytest.fixture
def tool():
    return BrowserOperatorTool.from_credentials({})


# ---------------------------------------------------------------------------
# Validation: missing top-level required params
# ---------------------------------------------------------------------------


class TestTopLevelValidation:
    def test_missing_cdp_url(self, tool):
        result = list(tool._invoke({"action": "navigate", "url": "https://example.com"}))
        assert len(result) == 1
        assert "cdp_url" in result[0].message.text
        assert "Error" in result[0].message.text

    def test_missing_action(self, tool):
        result = list(tool._invoke({"cdp_url": VALID_CDP_URL}))
        assert len(result) == 1
        assert "action" in result[0].message.text
        assert "Error" in result[0].message.text

    def test_unknown_action(self, tool):
        result = list(tool._invoke({"cdp_url": VALID_CDP_URL, "action": "explode"}))
        assert len(result) == 1
        assert "Unknown action" in result[0].message.text
        assert "explode" in result[0].message.text

    @patch("tools.browser_operator.cdp_page")
    def test_cdp_connection_error_propagates(self, mock_cdp, tool):
        mock_cdp.side_effect = CdpConnectionError("refused")
        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "navigate",
            "url": "https://example.com",
        }))
        assert "Error" in result[0].message.text
        assert "refused" in result[0].message.text

    @patch("tools.browser_operator.cdp_page")
    def test_generic_exception_returns_error_message(self, mock_cdp, tool):
        page = MagicMock()
        page.goto.side_effect = RuntimeError("browser crashed")
        mock_cdp.return_value = _mock_cdp_page(page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "navigate",
            "url": "https://example.com",
        }))

        assert len(result) == 1
        assert "RuntimeError" in result[0].message.text
        assert "browser crashed" in result[0].message.text
        assert "navigate" in result[0].message.text

    def test_whitespace_cdp_url_rejected(self, tool):
        result = list(tool._invoke({
            "cdp_url": "   ",
            "action": "navigate",
            "url": "https://example.com",
        }))
        assert "Error" in result[0].message.text
        assert "cdp_url" in result[0].message.text

    def test_empty_string_action_rejected(self, tool):
        result = list(tool._invoke({"cdp_url": VALID_CDP_URL, "action": ""}))
        assert "Error" in result[0].message.text
        assert "action" in result[0].message.text


# ---------------------------------------------------------------------------
# _pre_validate unit tests
# ---------------------------------------------------------------------------


class TestPreValidate:
    def test_navigate_requires_url(self):
        assert _pre_validate("navigate", {}) is not None

    def test_navigate_requires_http_scheme(self):
        result = _pre_validate("navigate", {"url": "ftp://x"})
        assert result is not None
        assert "http" in result

    def test_navigate_valid(self):
        assert _pre_validate("navigate", {"url": "https://x.com"}) is None

    def test_click_requires_selector(self):
        assert _pre_validate("click", {}) is not None

    def test_click_valid(self):
        assert _pre_validate("click", {"selector": ".btn"}) is None

    def test_hover_requires_selector(self):
        assert _pre_validate("hover", {}) is not None

    def test_type_requires_selector(self):
        assert _pre_validate("type", {"text": "hi"}) is not None

    def test_type_requires_text(self):
        assert _pre_validate("type", {"selector": "#q"}) is not None

    def test_type_valid(self):
        assert _pre_validate("type", {"selector": "#q", "text": "hi"}) is None

    def test_fill_requires_selector(self):
        assert _pre_validate("fill", {}) is not None

    def test_fill_allows_empty_text(self):
        assert _pre_validate("fill", {"selector": "#f"}) is None

    def test_select_requires_selector(self):
        assert _pre_validate("select", {"value": "v"}) is not None

    def test_select_requires_value(self):
        assert _pre_validate("select", {"selector": "s"}) is not None

    def test_select_valid(self):
        assert _pre_validate("select", {"selector": "s", "value": "v"}) is None

    def test_press_key_requires_key(self):
        assert _pre_validate("press_key", {}) is not None

    def test_wait_requires_selector(self):
        assert _pre_validate("wait", {}) is not None

    def test_actions_without_required_params_return_none(self):
        for action in ("screenshot", "go_back", "go_forward", "reload",
                        "wait_navigation", "get_text", "get_html"):
            assert _pre_validate(action, {}) is None, f"{action} should not require params"

    def test_whitespace_selector_rejected(self):
        assert _pre_validate("click", {"selector": "   "}) is not None

    def test_whitespace_url_rejected(self):
        assert _pre_validate("navigate", {"url": "  "}) is not None


# ---------------------------------------------------------------------------
# navigate
# ---------------------------------------------------------------------------


class TestNavigateAction:
    @patch("tools.browser_operator.cdp_page")
    def test_navigate_success(self, mock_cdp, tool):
        page = MagicMock()
        page.url = "https://example.com"
        page.title.return_value = "Example Domain"
        mock_cdp.return_value = _mock_cdp_page(page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "navigate",
            "url": "https://example.com",
        }))

        assert len(result) == 1
        assert "Navigation successful" in result[0].message.text
        assert "https://example.com" in result[0].message.text
        assert "Example Domain" in result[0].message.text
        page.goto.assert_called_once_with(
            "https://example.com", timeout=30000.0, wait_until="domcontentloaded"
        )

    def test_navigate_missing_url(self, tool):
        result = list(tool._invoke({"cdp_url": VALID_CDP_URL, "action": "navigate"}))
        assert "Error" in result[0].message.text
        assert "url" in result[0].message.text

    def test_navigate_invalid_url_scheme(self, tool):
        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "navigate",
            "url": "ftp://example.com",
        }))
        assert "Error" in result[0].message.text
        assert "http" in result[0].message.text

    @patch("tools.browser_operator.cdp_page")
    def test_navigate_custom_timeout(self, mock_cdp, tool):
        page = MagicMock()
        page.url = "https://slow.com"
        page.title.return_value = "Slow"
        mock_cdp.return_value = _mock_cdp_page(page)

        list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "navigate",
            "url": "https://slow.com",
            "timeout_ms": 60000,
        }))

        page.goto.assert_called_once_with(
            "https://slow.com", timeout=60000.0, wait_until="domcontentloaded"
        )


# ---------------------------------------------------------------------------
# click
# ---------------------------------------------------------------------------


class TestClickAction:
    @patch("tools.browser_operator.cdp_page")
    def test_click_success(self, mock_cdp, tool):
        page = MagicMock()
        mock_cdp.return_value = _mock_cdp_page(page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "click",
            "selector": ".submit-btn",
        }))

        assert "Clicked" in result[0].message.text
        assert ".submit-btn" in result[0].message.text
        page.click.assert_called_once_with(".submit-btn")

    @patch("tools.browser_operator.cdp_page")
    def test_click_element_not_found(self, mock_cdp, tool):
        page = MagicMock()
        page.wait_for_selector.side_effect = PlaywrightTimeout("timeout")
        mock_cdp.return_value = _mock_cdp_page(page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "click",
            "selector": ".missing",
            "timeout_ms": 100,
        }))

        assert "not found" in result[0].message.text.lower()

    def test_click_missing_selector(self, tool):
        result = list(tool._invoke({"cdp_url": VALID_CDP_URL, "action": "click"}))
        assert "Error" in result[0].message.text
        assert "selector" in result[0].message.text


# ---------------------------------------------------------------------------
# type
# ---------------------------------------------------------------------------


class TestTypeAction:
    @patch("tools.browser_operator.cdp_page")
    def test_type_success(self, mock_cdp, tool):
        page = MagicMock()
        mock_cdp.return_value = _mock_cdp_page(page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "type",
            "selector": "#search",
            "text": "hello world",
        }))

        assert "Typed" in result[0].message.text
        page.type.assert_called_once_with("#search", "hello world")

    def test_type_missing_selector(self, tool):
        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "type",
            "text": "hello",
        }))
        assert "Error" in result[0].message.text
        assert "selector" in result[0].message.text

    def test_type_missing_text(self, tool):
        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "type",
            "selector": "#q",
        }))
        assert "Error" in result[0].message.text
        assert "text" in result[0].message.text


# ---------------------------------------------------------------------------
# fill
# ---------------------------------------------------------------------------


class TestFillAction:
    @patch("tools.browser_operator.cdp_page")
    def test_fill_success(self, mock_cdp, tool):
        page = MagicMock()
        mock_cdp.return_value = _mock_cdp_page(page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "fill",
            "selector": "input[name='email']",
            "text": "user@example.com",
        }))

        assert "Filled" in result[0].message.text
        page.fill.assert_called_once_with("input[name='email']", "user@example.com")

    @patch("tools.browser_operator.cdp_page")
    def test_fill_empty_text_allowed(self, mock_cdp, tool):
        page = MagicMock()
        mock_cdp.return_value = _mock_cdp_page(page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "fill",
            "selector": "#field",
            "text": "",
        }))

        # fill with empty text is valid (clears field)
        assert "Filled" in result[0].message.text
        page.fill.assert_called_once_with("#field", "")

    def test_fill_missing_selector(self, tool):
        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "fill",
            "text": "value",
        }))
        assert "Error" in result[0].message.text
        assert "selector" in result[0].message.text


# ---------------------------------------------------------------------------
# select
# ---------------------------------------------------------------------------


class TestSelectAction:
    @patch("tools.browser_operator.cdp_page")
    def test_select_success(self, mock_cdp, tool):
        page = MagicMock()
        page.select_option.return_value = ["US"]
        mock_cdp.return_value = _mock_cdp_page(page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "select",
            "selector": "select#country",
            "value": "US",
        }))

        assert "Selected" in result[0].message.text
        page.select_option.assert_called_once_with("select#country", value="US")

    @patch("tools.browser_operator.cdp_page")
    def test_select_no_matching_option(self, mock_cdp, tool):
        page = MagicMock()
        page.select_option.return_value = []
        mock_cdp.return_value = _mock_cdp_page(page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "select",
            "selector": "select#x",
            "value": "INVALID",
        }))

        assert "No option" in result[0].message.text

    def test_select_missing_value(self, tool):
        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "select",
            "selector": "select#x",
        }))
        assert "Error" in result[0].message.text
        assert "value" in result[0].message.text

    def test_select_missing_selector(self, tool):
        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "select",
            "value": "US",
        }))
        assert "Error" in result[0].message.text
        assert "selector" in result[0].message.text


# ---------------------------------------------------------------------------
# press_key
# ---------------------------------------------------------------------------


class TestPressKeyAction:
    @patch("tools.browser_operator.cdp_page")
    def test_press_key_success(self, mock_cdp, tool):
        page = MagicMock()
        mock_cdp.return_value = _mock_cdp_page(page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "press_key",
            "key": "Enter",
        }))

        assert "Enter" in result[0].message.text
        page.keyboard.press.assert_called_once_with("Enter")

    def test_press_key_missing_key(self, tool):
        result = list(tool._invoke({"cdp_url": VALID_CDP_URL, "action": "press_key"}))
        assert "Error" in result[0].message.text
        assert "key" in result[0].message.text


# ---------------------------------------------------------------------------
# hover
# ---------------------------------------------------------------------------


class TestHoverAction:
    @patch("tools.browser_operator.cdp_page")
    def test_hover_success(self, mock_cdp, tool):
        page = MagicMock()
        mock_cdp.return_value = _mock_cdp_page(page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "hover",
            "selector": ".dropdown-trigger",
        }))

        assert "Hovered" in result[0].message.text
        page.hover.assert_called_once_with(".dropdown-trigger")

    def test_hover_missing_selector(self, tool):
        result = list(tool._invoke({"cdp_url": VALID_CDP_URL, "action": "hover"}))
        assert "Error" in result[0].message.text
        assert "selector" in result[0].message.text


# ---------------------------------------------------------------------------
# screenshot
# ---------------------------------------------------------------------------


class TestScreenshotAction:
    @patch("tools.browser_operator.cdp_page")
    def test_screenshot_returns_blob_and_text(self, mock_cdp, tool):
        page = MagicMock()
        page.screenshot.return_value = b"\x89PNG\r\n"
        page.url = "https://example.com"
        mock_cdp.return_value = _mock_cdp_page(page)

        result = list(tool._invoke({"cdp_url": VALID_CDP_URL, "action": "screenshot"}))

        # First message is blob, second is text confirmation
        assert len(result) == 2
        assert result[0].message.blob == b"\x89PNG\r\n"
        assert "https://example.com" in result[1].message.text
        page.screenshot.assert_called_once_with(full_page=False)

    @patch("tools.browser_operator.cdp_page")
    def test_screenshot_full_page(self, mock_cdp, tool):
        page = MagicMock()
        page.screenshot.return_value = b"PNG"
        page.url = "https://example.com"
        mock_cdp.return_value = _mock_cdp_page(page)

        list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "screenshot",
            "full_page": "true",
        }))

        page.screenshot.assert_called_once_with(full_page=True)


# ---------------------------------------------------------------------------
# get_text
# ---------------------------------------------------------------------------


class TestGetTextAction:
    @patch("tools.browser_operator.cdp_page")
    def test_get_body_text(self, mock_cdp, tool):
        page = MagicMock()
        page.inner_text.return_value = "Hello World"
        mock_cdp.return_value = _mock_cdp_page(page)

        result = list(tool._invoke({"cdp_url": VALID_CDP_URL, "action": "get_text"}))

        assert "Hello World" in result[0].message.text
        page.inner_text.assert_called_once_with("body")

    @patch("tools.browser_operator.cdp_page")
    def test_get_element_text(self, mock_cdp, tool):
        element = MagicMock()
        element.inner_text.return_value = "Button Text"
        page = MagicMock()
        page.query_selector.return_value = element
        mock_cdp.return_value = _mock_cdp_page(page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "get_text",
            "selector": ".btn",
        }))

        assert "Button Text" in result[0].message.text

    @patch("tools.browser_operator.cdp_page")
    def test_get_text_element_not_found(self, mock_cdp, tool):
        page = MagicMock()
        page.query_selector.return_value = None
        mock_cdp.return_value = _mock_cdp_page(page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "get_text",
            "selector": ".missing",
        }))

        assert "No element found" in result[0].message.text


# ---------------------------------------------------------------------------
# get_html
# ---------------------------------------------------------------------------


class TestGetHtmlAction:
    @patch("tools.browser_operator.cdp_page")
    def test_get_full_page_html(self, mock_cdp, tool):
        page = MagicMock()
        page.content.return_value = "<html><body>Hi</body></html>"
        mock_cdp.return_value = _mock_cdp_page(page)

        result = list(tool._invoke({"cdp_url": VALID_CDP_URL, "action": "get_html"}))

        assert "<html>" in result[0].message.text
        page.content.assert_called_once()

    @patch("tools.browser_operator.cdp_page")
    def test_get_element_html(self, mock_cdp, tool):
        element = MagicMock()
        element.inner_html.return_value = "<span>Hello</span>"
        page = MagicMock()
        page.query_selector.return_value = element
        mock_cdp.return_value = _mock_cdp_page(page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "get_html",
            "selector": "div.content",
        }))

        assert "<span>Hello</span>" in result[0].message.text

    @patch("tools.browser_operator.cdp_page")
    def test_get_html_element_not_found(self, mock_cdp, tool):
        page = MagicMock()
        page.query_selector.return_value = None
        mock_cdp.return_value = _mock_cdp_page(page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "get_html",
            "selector": ".missing",
        }))

        assert "No element found" in result[0].message.text


# ---------------------------------------------------------------------------
# wait
# ---------------------------------------------------------------------------


class TestWaitAction:
    @patch("tools.browser_operator.cdp_page")
    def test_wait_element_found(self, mock_cdp, tool):
        page = MagicMock()
        mock_cdp.return_value = _mock_cdp_page(page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "wait",
            "selector": ".loaded",
        }))

        assert "found" in result[0].message.text.lower()

    @patch("tools.browser_operator.cdp_page")
    def test_wait_timeout_returns_message_not_exception(self, mock_cdp, tool):
        page = MagicMock()
        page.wait_for_selector.side_effect = PlaywrightTimeout("timeout")
        mock_cdp.return_value = _mock_cdp_page(page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "wait",
            "selector": ".missing",
            "timeout_ms": 100,
        }))

        assert "not found" in result[0].message.text.lower()

    def test_wait_missing_selector(self, tool):
        result = list(tool._invoke({"cdp_url": VALID_CDP_URL, "action": "wait"}))
        assert "Error" in result[0].message.text
        assert "selector" in result[0].message.text


# ---------------------------------------------------------------------------
# wait_navigation
# ---------------------------------------------------------------------------


class TestWaitNavigationAction:
    @patch("tools.browser_operator.cdp_page")
    def test_wait_navigation_complete(self, mock_cdp, tool):
        page = MagicMock()
        page.url = "https://example.com/dashboard"
        page.title.return_value = "Dashboard"
        mock_cdp.return_value = _mock_cdp_page(page)

        result = list(tool._invoke({"cdp_url": VALID_CDP_URL, "action": "wait_navigation"}))

        assert "complete" in result[0].message.text.lower()
        assert "https://example.com/dashboard" in result[0].message.text

    @patch("tools.browser_operator.cdp_page")
    def test_wait_navigation_timeout(self, mock_cdp, tool):
        page = MagicMock()
        page.url = "https://example.com"
        page.wait_for_load_state.side_effect = PlaywrightTimeout("timeout")
        mock_cdp.return_value = _mock_cdp_page(page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "wait_navigation",
            "timeout_ms": 100,
        }))

        assert "did not complete" in result[0].message.text.lower()


# ---------------------------------------------------------------------------
# go_back
# ---------------------------------------------------------------------------


class TestGoBackAction:
    @patch("tools.browser_operator.cdp_page")
    def test_go_back_success(self, mock_cdp, tool):
        page = MagicMock()
        page.url = "https://example.com"
        mock_cdp.return_value = _mock_cdp_page(page)

        result = list(tool._invoke({"cdp_url": VALID_CDP_URL, "action": "go_back"}))

        assert "back" in result[0].message.text.lower()
        page.go_back.assert_called_once()


# ---------------------------------------------------------------------------
# go_forward
# ---------------------------------------------------------------------------


class TestGoForwardAction:
    @patch("tools.browser_operator.cdp_page")
    def test_go_forward_success(self, mock_cdp, tool):
        page = MagicMock()
        page.url = "https://example.com/next"
        mock_cdp.return_value = _mock_cdp_page(page)

        result = list(tool._invoke({"cdp_url": VALID_CDP_URL, "action": "go_forward"}))

        assert "forward" in result[0].message.text.lower()
        page.go_forward.assert_called_once()


# ---------------------------------------------------------------------------
# reload
# ---------------------------------------------------------------------------


class TestReloadAction:
    @patch("tools.browser_operator.cdp_page")
    def test_reload_success(self, mock_cdp, tool):
        page = MagicMock()
        page.url = "https://example.com"
        mock_cdp.return_value = _mock_cdp_page(page)

        result = list(tool._invoke({"cdp_url": VALID_CDP_URL, "action": "reload"}))

        assert "reload" in result[0].message.text.lower()
        page.reload.assert_called_once_with(wait_until="domcontentloaded")


# ---------------------------------------------------------------------------
# Edge cases
# ---------------------------------------------------------------------------


class TestEdgeCases:
    @patch("tools.browser_operator.cdp_page")
    def test_click_with_zero_timeout_skips_wait(self, mock_cdp, tool):
        page = MagicMock()
        mock_cdp.return_value = _mock_cdp_page(page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "click",
            "selector": ".btn",
            "timeout_ms": 0,
        }))

        page.wait_for_selector.assert_not_called()
        page.click.assert_called_once_with(".btn")
        assert "Clicked" in result[0].message.text

    def test_type_explicit_empty_string_rejected(self, tool):
        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "type",
            "selector": "#q",
            "text": "",
        }))
        assert "Error" in result[0].message.text
        assert "text" in result[0].message.text

    @patch("tools.browser_operator.cdp_page")
    def test_get_text_empty_body_returns_empty_marker(self, mock_cdp, tool):
        page = MagicMock()
        page.inner_text.return_value = ""
        mock_cdp.return_value = _mock_cdp_page(page)

        result = list(tool._invoke({"cdp_url": VALID_CDP_URL, "action": "get_text"}))

        assert result[0].message.text == "(empty)"

    @patch("tools.browser_operator.cdp_page")
    def test_get_html_empty_returns_empty_marker(self, mock_cdp, tool):
        page = MagicMock()
        page.content.return_value = ""
        mock_cdp.return_value = _mock_cdp_page(page)

        result = list(tool._invoke({"cdp_url": VALID_CDP_URL, "action": "get_html"}))

        assert result[0].message.text == "(empty)"

    @patch("tools.browser_operator.cdp_page")
    def test_navigate_invalid_timeout_uses_default(self, mock_cdp, tool):
        page = MagicMock()
        page.url = "https://example.com"
        page.title.return_value = "Test"
        mock_cdp.return_value = _mock_cdp_page(page)

        list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "navigate",
            "url": "https://example.com",
            "timeout_ms": "abc",
        }))

        page.goto.assert_called_once_with(
            "https://example.com", timeout=30000.0, wait_until="domcontentloaded"
        )

    def test_navigate_whitespace_url_rejected(self, tool):
        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "navigate",
            "url": "  ",
        }))
        assert "Error" in result[0].message.text

    def test_click_whitespace_selector_rejected(self, tool):
        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "click",
            "selector": "   ",
        }))
        assert "Error" in result[0].message.text

    def test_malformed_cdp_url_returns_error(self, tool):
        """Malformed CDP URL (not ws/wss/http/https) returns validation error."""
        result = list(tool._invoke({
            "cdp_url": "ftp://malformed:9222",
            "action": "navigate",
            "url": "https://example.com",
        }))
        assert len(result) == 1
        assert "Error" in result[0].message.text

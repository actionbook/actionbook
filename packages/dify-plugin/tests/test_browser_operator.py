"""Tests for the unified BrowserOperatorTool."""

import ipaddress
import sys
from pathlib import Path
from urllib.parse import urlparse
from unittest.mock import MagicMock, patch

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent))

from playwright.sync_api import TimeoutError as PlaywrightTimeout

from tools.browser_operator import (
    BrowserOperatorTool,
    _is_ssrf_target,
    _pre_validate,
    _render_snapshot_node,
)
from utils.connection_pool import ConnectionNotFound, ConnectionUnhealthy

VALID_CDP_URL = "ws://localhost:9222"


def _setup_pool_mock(mock_pool: MagicMock, page: MagicMock) -> None:
    """Configure a pool mock so that connect + get_page return the given page."""
    mock_pool.get_page.return_value = page


def _extract_message_url(message: str) -> str:
    """Extract URL value from tool output lines like 'URL: https://example.com'."""
    for line in message.splitlines():
        if "URL: " in line:
            return line.split("URL: ", 1)[1].strip()
    return ""


@pytest.fixture
def tool():
    return BrowserOperatorTool.from_credentials({})


# ---------------------------------------------------------------------------
# Validation: missing top-level required params
# ---------------------------------------------------------------------------


class TestTopLevelValidation:
    def test_missing_both_session_id_and_cdp_url(self, tool):
        result = list(tool._invoke({"action": "navigate", "url": "https://example.com"}))
        assert len(result) == 1
        assert "session_id" in result[0].message.text or "cdp_url" in result[0].message.text
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

    @patch("tools.browser_operator.pool")
    def test_cdp_connection_error_propagates(self, mock_pool, tool):
        mock_pool.connect.side_effect = RuntimeError("refused")
        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "navigate",
            "url": "https://example.com",
        }))
        assert "Error" in result[0].message.text
        assert "refused" in result[0].message.text

    @patch("tools.browser_operator.pool")
    def test_generic_exception_returns_error_message(self, mock_pool, tool):
        page = MagicMock()
        page.goto.side_effect = RuntimeError("browser crashed")
        _setup_pool_mock(mock_pool, page)

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
        for action in ("snapshot", "go_back", "go_forward", "reload",
                        "wait_navigation", "get_text", "get_html"):
            assert _pre_validate(action, {}) is None, f"{action} should not require params"

    def test_whitespace_selector_rejected(self):
        assert _pre_validate("click", {"selector": "   "}) is not None

    def test_whitespace_url_rejected(self):
        assert _pre_validate("navigate", {"url": "  "}) is not None


class TestSsrfProtection:
    def test_blocks_localhost_with_trailing_dot(self):
        assert _is_ssrf_target("http://localhost./admin")

    def test_blocks_short_ipv4_loopback_notation(self):
        assert _is_ssrf_target("http://127.1/internal")

    def test_blocks_integer_ipv4_loopback_notation(self):
        assert _is_ssrf_target("http://2130706433/internal")

    def test_blocks_hex_ipv4_loopback_notation(self):
        assert _is_ssrf_target("http://0x7f000001/internal")

    @patch("tools.browser_operator._resolve_host_ips")
    def test_blocks_dns_resolved_private_ip(self, mock_resolve):
        mock_resolve.return_value = {ipaddress.ip_address("127.0.0.1")}
        assert _is_ssrf_target("http://example.test/internal")

    @patch("tools.browser_operator._resolve_host_ips")
    def test_allows_dns_resolved_public_ip(self, mock_resolve):
        mock_resolve.return_value = {ipaddress.ip_address("8.8.8.8")}
        assert not _is_ssrf_target("http://example.test/public")


# ---------------------------------------------------------------------------
# navigate
# ---------------------------------------------------------------------------


class TestNavigateAction:
    @patch("tools.browser_operator.pool")
    def test_navigate_success(self, mock_pool, tool):
        page = MagicMock()
        page.url = "https://example.com"
        page.title.return_value = "Example Domain"
        _setup_pool_mock(mock_pool, page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "navigate",
            "url": "https://example.com",
        }))

        assert len(result) == 1
        assert "Navigation successful" in result[0].message.text
        assert urlparse(_extract_message_url(result[0].message.text)).hostname == "example.com"
        assert "Example Domain" in result[0].message.text
        page.goto.assert_called_once_with(
            "https://example.com", timeout=30000.0, wait_until="domcontentloaded"
        )

    @patch("tools.browser_operator.pool")
    def test_navigate_custom_timeout(self, mock_pool, tool):
        page = MagicMock()
        page.url = "https://slow.com"
        page.title.return_value = "Slow"
        _setup_pool_mock(mock_pool, page)

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
    @patch("tools.browser_operator.pool")
    def test_click_success(self, mock_pool, tool):
        page = MagicMock()
        _setup_pool_mock(mock_pool, page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "click",
            "selector": ".submit-btn",
        }))

        assert "Clicked" in result[0].message.text
        assert ".submit-btn" in result[0].message.text
        page.click.assert_called_once_with(".submit-btn")

    @patch("tools.browser_operator.pool")
    def test_click_element_not_found_suggests_snapshot_and_stop(self, mock_pool, tool):
        page = MagicMock()
        page.wait_for_selector.side_effect = PlaywrightTimeout("timeout")
        _setup_pool_mock(mock_pool, page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "click",
            "selector": ".missing",
            "timeout_ms": 100,
        }))

        assert len(result) == 1
        text = result[0].message.text
        assert "not found" in text.lower()
        assert "snapshot" in text.lower()
        assert "browser_stop_session" in text
        assert "[ref=eN]" in text
        # No automatic snapshot/retry should happen
        page.evaluate.assert_not_called()

    @patch("tools.browser_operator.pool")
    def test_click_generic_exception_suggests_refs_and_stop(self, mock_pool, tool):
        page = MagicMock()
        page.click.side_effect = RuntimeError("element detached")
        _setup_pool_mock(mock_pool, page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "click",
            "selector": ".btn",
            "timeout_ms": 0,
        }))

        assert len(result) == 1
        text = result[0].message.text
        assert "Click failed" in text
        assert "[ref=eN]" in text
        assert "browser_stop_session" in text
        page.evaluate.assert_not_called()

    @patch("tools.browser_operator.pool")
    def test_click_default_timeout_is_10s(self, mock_pool, tool):
        """Click uses 10s default timeout, not the global 30s."""
        page = MagicMock()
        _setup_pool_mock(mock_pool, page)

        list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "click",
            "selector": ".btn",
        }))

        page.wait_for_selector.assert_called_once_with(".btn", timeout=10000.0)



# ---------------------------------------------------------------------------
# type
# ---------------------------------------------------------------------------


class TestTypeAction:
    @patch("tools.browser_operator.pool")
    def test_type_success(self, mock_pool, tool):
        page = MagicMock()
        _setup_pool_mock(mock_pool, page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "type",
            "selector": "#search",
            "text": "hello world",
        }))

        assert "Typed" in result[0].message.text
        assert "11 characters" in result[0].message.text
        page.type.assert_called_once_with("#search", "hello world")



# ---------------------------------------------------------------------------
# fill
# ---------------------------------------------------------------------------


class TestFillAction:
    @patch("tools.browser_operator.pool")
    def test_fill_success(self, mock_pool, tool):
        page = MagicMock()
        _setup_pool_mock(mock_pool, page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "fill",
            "selector": "input[name='email']",
            "text": "user@example.com",
        }))

        assert "Filled" in result[0].message.text
        assert "16 chars" in result[0].message.text
        page.fill.assert_called_once_with("input[name='email']", "user@example.com")

    @patch("tools.browser_operator.pool")
    def test_fill_empty_text_allowed(self, mock_pool, tool):
        page = MagicMock()
        _setup_pool_mock(mock_pool, page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "fill",
            "selector": "#field",
            "text": "",
        }))

        # fill with empty text is valid (clears field)
        assert "Filled" in result[0].message.text
        page.fill.assert_called_once_with("#field", "")



# ---------------------------------------------------------------------------
# select
# ---------------------------------------------------------------------------


class TestSelectAction:
    @patch("tools.browser_operator.pool")
    def test_select_success(self, mock_pool, tool):
        page = MagicMock()
        page.select_option.return_value = ["US"]
        _setup_pool_mock(mock_pool, page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "select",
            "selector": "select#country",
            "value": "US",
        }))

        assert "Selected" in result[0].message.text
        page.select_option.assert_called_once_with("select#country", value="US")

    @patch("tools.browser_operator.pool")
    def test_select_no_matching_option(self, mock_pool, tool):
        page = MagicMock()
        page.select_option.return_value = []
        _setup_pool_mock(mock_pool, page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "select",
            "selector": "select#x",
            "value": "INVALID",
        }))

        assert "No option" in result[0].message.text



# ---------------------------------------------------------------------------
# press_key
# ---------------------------------------------------------------------------


class TestPressKeyAction:
    @patch("tools.browser_operator.pool")
    def test_press_key_success(self, mock_pool, tool):
        page = MagicMock()
        _setup_pool_mock(mock_pool, page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "press_key",
            "key": "Enter",
        }))

        assert "Enter" in result[0].message.text
        page.keyboard.press.assert_called_once_with("Enter")



# ---------------------------------------------------------------------------
# hover
# ---------------------------------------------------------------------------


class TestHoverAction:
    @patch("tools.browser_operator.pool")
    def test_hover_success(self, mock_pool, tool):
        page = MagicMock()
        _setup_pool_mock(mock_pool, page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "hover",
            "selector": ".dropdown-trigger",
        }))

        assert "Hovered" in result[0].message.text
        page.hover.assert_called_once_with(".dropdown-trigger")



# ---------------------------------------------------------------------------
# get_text
# ---------------------------------------------------------------------------


class TestGetTextAction:
    @patch("tools.browser_operator.pool")
    def test_get_body_text(self, mock_pool, tool):
        page = MagicMock()
        page.inner_text.return_value = "Hello World"
        _setup_pool_mock(mock_pool, page)

        result = list(tool._invoke({"cdp_url": VALID_CDP_URL, "action": "get_text"}))

        assert "Hello World" in result[0].message.text
        page.inner_text.assert_called_once_with("body")

    @patch("tools.browser_operator.pool")
    def test_get_element_text(self, mock_pool, tool):
        page = MagicMock()
        page.inner_text.return_value = "Button Text"
        _setup_pool_mock(mock_pool, page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "get_text",
            "selector": ".btn",
        }))

        assert "Button Text" in result[0].message.text
        page.inner_text.assert_called_once_with(".btn")

    @patch("tools.browser_operator.pool")
    def test_get_text_element_not_found(self, mock_pool, tool):
        page = MagicMock()
        page.inner_text.side_effect = Exception("Element not found")
        _setup_pool_mock(mock_pool, page)

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
    @patch("tools.browser_operator.pool")
    def test_get_full_page_html(self, mock_pool, tool):
        page = MagicMock()
        page.content.return_value = "<html><body>Hi</body></html>"
        _setup_pool_mock(mock_pool, page)

        result = list(tool._invoke({"cdp_url": VALID_CDP_URL, "action": "get_html"}))

        assert "<html>" in result[0].message.text
        page.content.assert_called_once()

    @patch("tools.browser_operator.pool")
    def test_get_element_html(self, mock_pool, tool):
        page = MagicMock()
        page.inner_html.return_value = "<span>Hello</span>"
        _setup_pool_mock(mock_pool, page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "get_html",
            "selector": "div.content",
        }))

        assert "<span>Hello</span>" in result[0].message.text
        page.inner_html.assert_called_once_with("div.content")

    @patch("tools.browser_operator.pool")
    def test_get_html_element_not_found(self, mock_pool, tool):
        page = MagicMock()
        page.inner_html.side_effect = Exception("Element not found")
        _setup_pool_mock(mock_pool, page)

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
    @patch("tools.browser_operator.pool")
    def test_wait_element_found(self, mock_pool, tool):
        page = MagicMock()
        _setup_pool_mock(mock_pool, page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "wait",
            "selector": ".loaded",
        }))

        assert "found" in result[0].message.text.lower()

    @patch("tools.browser_operator.pool")
    def test_wait_timeout_returns_message_not_exception(self, mock_pool, tool):
        page = MagicMock()
        page.wait_for_selector.side_effect = PlaywrightTimeout("timeout")
        _setup_pool_mock(mock_pool, page)

        result = list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "wait",
            "selector": ".missing",
            "timeout_ms": 100,
        }))

        assert "not found" in result[0].message.text.lower()



# ---------------------------------------------------------------------------
# wait_navigation
# ---------------------------------------------------------------------------


class TestWaitNavigationAction:
    @patch("tools.browser_operator.pool")
    def test_wait_navigation_complete(self, mock_pool, tool):
        page = MagicMock()
        page.url = "https://example.com/dashboard"
        page.title.return_value = "Dashboard"
        _setup_pool_mock(mock_pool, page)

        result = list(tool._invoke({"cdp_url": VALID_CDP_URL, "action": "wait_navigation"}))

        assert "complete" in result[0].message.text.lower()
        assert "https://example.com/dashboard" in result[0].message.text

    @patch("tools.browser_operator.pool")
    def test_wait_navigation_timeout(self, mock_pool, tool):
        page = MagicMock()
        page.url = "https://example.com"
        page.wait_for_load_state.side_effect = PlaywrightTimeout("timeout")
        _setup_pool_mock(mock_pool, page)

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
    @patch("tools.browser_operator.pool")
    def test_go_back_success(self, mock_pool, tool):
        page = MagicMock()
        page.url = "https://example.com"
        _setup_pool_mock(mock_pool, page)

        result = list(tool._invoke({"cdp_url": VALID_CDP_URL, "action": "go_back"}))

        assert "back" in result[0].message.text.lower()
        page.go_back.assert_called_once()


# ---------------------------------------------------------------------------
# go_forward
# ---------------------------------------------------------------------------


class TestGoForwardAction:
    @patch("tools.browser_operator.pool")
    def test_go_forward_success(self, mock_pool, tool):
        page = MagicMock()
        page.url = "https://example.com/next"
        _setup_pool_mock(mock_pool, page)

        result = list(tool._invoke({"cdp_url": VALID_CDP_URL, "action": "go_forward"}))

        assert "forward" in result[0].message.text.lower()
        page.go_forward.assert_called_once()


# ---------------------------------------------------------------------------
# reload
# ---------------------------------------------------------------------------


class TestReloadAction:
    @patch("tools.browser_operator.pool")
    def test_reload_success(self, mock_pool, tool):
        page = MagicMock()
        page.url = "https://example.com"
        _setup_pool_mock(mock_pool, page)

        result = list(tool._invoke({"cdp_url": VALID_CDP_URL, "action": "reload"}))

        assert "reload" in result[0].message.text.lower()
        page.reload.assert_called_once_with(wait_until="domcontentloaded")


# ---------------------------------------------------------------------------
# Edge cases
# ---------------------------------------------------------------------------


class TestEdgeCases:
    @patch("tools.browser_operator.pool")
    def test_click_with_zero_timeout_skips_wait(self, mock_pool, tool):
        page = MagicMock()
        _setup_pool_mock(mock_pool, page)

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

    @patch("tools.browser_operator.pool")
    def test_get_text_empty_body_returns_empty_marker(self, mock_pool, tool):
        page = MagicMock()
        page.inner_text.return_value = ""
        _setup_pool_mock(mock_pool, page)

        result = list(tool._invoke({"cdp_url": VALID_CDP_URL, "action": "get_text"}))

        assert result[0].message.text == "(empty)"

    @patch("tools.browser_operator.pool")
    def test_get_html_empty_returns_empty_marker(self, mock_pool, tool):
        page = MagicMock()
        page.content.return_value = ""
        _setup_pool_mock(mock_pool, page)

        result = list(tool._invoke({"cdp_url": VALID_CDP_URL, "action": "get_html"}))

        assert result[0].message.text == "(empty)"

    @patch("tools.browser_operator.pool")
    def test_navigate_invalid_timeout_uses_default(self, mock_pool, tool):
        page = MagicMock()
        page.url = "https://example.com"
        page.title.return_value = "Test"
        _setup_pool_mock(mock_pool, page)

        list(tool._invoke({
            "cdp_url": VALID_CDP_URL,
            "action": "navigate",
            "url": "https://example.com",
            "timeout_ms": "abc",
        }))

        page.goto.assert_called_once_with(
            "https://example.com", timeout=30000.0, wait_until="domcontentloaded"
        )

    def test_malformed_cdp_url_returns_error(self, tool):
        """Malformed CDP URL (not ws/wss/http/https) returns validation error."""
        result = list(tool._invoke({
            "cdp_url": "ftp://malformed:9222",
            "action": "navigate",
            "url": "https://example.com",
        }))
        assert len(result) == 1
        assert "Error" in result[0].message.text


# ---------------------------------------------------------------------------
# Session ID (connection pool) tests
# ---------------------------------------------------------------------------


class TestSessionIdPath:
    """Tests for the session_id-based pool lookup path."""

    @patch("tools.browser_operator.pool")
    def test_session_id_navigate_success(self, mock_pool, tool):
        page = MagicMock()
        page.url = "https://example.com"
        page.title.return_value = "Example Domain"
        mock_pool.get_page.return_value = page

        result = list(tool._invoke({
            "session_id": "sess-abc",
            "action": "navigate",
            "url": "https://example.com",
        }))

        assert len(result) == 1
        assert "Navigation successful" in result[0].message.text
        mock_pool.get_page.assert_called_once_with("sess-abc")

    @patch("tools.browser_operator.pool")
    def test_session_id_not_found_no_fallback(self, mock_pool, tool):
        mock_pool.get_page.side_effect = ConnectionNotFound("no connection")

        result = list(tool._invoke({
            "session_id": "sess-missing",
            "action": "navigate",
            "url": "https://example.com",
        }))

        assert "Error" in result[0].message.text
        assert "no connection" in result[0].message.text
        assert "cdp_url" in result[0].message.text

    @patch("tools.browser_operator.pool")
    def test_session_id_not_found_falls_back_to_cdp_url(self, mock_pool, tool):
        """When session_id is not found but cdp_url is provided, reconnect via pool.

        Reconnect must forward the provider metadata cached at session creation
        so that browser_stop_session can correctly stop the remote session.
        """
        page = MagicMock()
        page.url = "https://example.com"
        page.title.return_value = "Example"

        # get_session_info returns cached provider metadata
        mock_pool.get_session_info.return_value = ("hyperbrowser", "hb-key-123")

        # First call (get_page for session_id) fails, then reconnect succeeds
        mock_pool.get_page.side_effect = [
            ConnectionNotFound("not in pool"),
            page,
        ]

        result = list(tool._invoke({
            "session_id": "sess-missing",
            "cdp_url": VALID_CDP_URL,
            "action": "navigate",
            "url": "https://example.com",
        }))

        assert "Navigation successful" in result[0].message.text
        mock_pool.connect.assert_called_once_with(
            "sess-missing", VALID_CDP_URL,
            provider_name="hyperbrowser",
            api_key="hb-key-123",
        )

    @patch("tools.browser_operator.pool")
    def test_session_id_multi_step_operations(self, mock_pool, tool):
        """Simulate multiple operations on the same session_id."""
        page = MagicMock()
        page.url = "https://example.com"
        page.title.return_value = "Example"
        page.inner_text.return_value = "Hello"
        mock_pool.get_page.return_value = page

        # Step 1: navigate
        r1 = list(tool._invoke({
            "session_id": "sess-multi",
            "action": "navigate",
            "url": "https://example.com",
        }))
        assert "Navigation successful" in r1[0].message.text

        # Step 2: fill
        r2 = list(tool._invoke({
            "session_id": "sess-multi",
            "action": "fill",
            "selector": "#name",
            "text": "Test",
        }))
        assert "Filled" in r2[0].message.text

        # Step 3: click
        r3 = list(tool._invoke({
            "session_id": "sess-multi",
            "action": "click",
            "selector": ".submit",
        }))
        assert "Clicked" in r3[0].message.text

        assert mock_pool.get_page.call_count == 3

    def test_session_id_only_whitespace_treated_as_empty(self, tool):
        """Whitespace-only session_id should not be used."""
        result = list(tool._invoke({
            "session_id": "   ",
            "action": "navigate",
            "url": "https://example.com",
        }))
        assert "Error" in result[0].message.text


# ---------------------------------------------------------------------------
# snapshot
# ---------------------------------------------------------------------------


class TestSnapshotAction:
    @patch("tools.browser_operator.pool")
    def test_snapshot_returns_accessibility_tree(self, mock_pool, tool):
        page = MagicMock()
        page.url = "https://example.com"
        page.evaluate.return_value = {
            "tree": {
                "role": "generic",
                "children": [
                    {"role": "heading", "name": "Welcome", "ref": "e1", "level": 1},
                    {"role": "button", "name": "Submit", "ref": "e2"},
                ],
            },
            "refCount": 2,
        }
        _setup_pool_mock(mock_pool, page)

        result = list(tool._invoke({"cdp_url": VALID_CDP_URL, "action": "snapshot"}))

        assert len(result) == 1
        text = result[0].message.text
        assert urlparse(_extract_message_url(text)).hostname == "example.com"
        assert "Interactive elements: 2" in text
        assert 'heading "Welcome"' in text
        assert "[ref=e1]" in text
        assert "[level=1]" in text
        assert 'button "Submit"' in text
        assert "[ref=e2]" in text

    @patch("tools.browser_operator.pool")
    def test_snapshot_empty_page(self, mock_pool, tool):
        page = MagicMock()
        page.url = "about:blank"
        page.evaluate.return_value = {"tree": None, "refCount": 0}
        _setup_pool_mock(mock_pool, page)

        result = list(tool._invoke({"cdp_url": VALID_CDP_URL, "action": "snapshot"}))

        assert len(result) == 1
        assert "empty" in result[0].message.text.lower()

    @patch("tools.browser_operator.pool")
    def test_snapshot_via_session_id(self, mock_pool, tool):
        page = MagicMock()
        page.url = "https://example.com"
        page.evaluate.return_value = {
            "tree": {"role": "button", "name": "OK", "ref": "e1"},
            "refCount": 1,
        }
        mock_pool.get_page.return_value = page

        result = list(tool._invoke({
            "session_id": "sess-snap",
            "action": "snapshot",
        }))

        assert len(result) == 1
        assert 'button "OK"' in result[0].message.text
        mock_pool.get_page.assert_called_once_with("sess-snap")

    def test_snapshot_registered_in_handlers(self):
        from tools.browser_operator import _HANDLERS
        assert "snapshot" in _HANDLERS


# ---------------------------------------------------------------------------
# _render_snapshot_node unit tests
# ---------------------------------------------------------------------------


class TestRenderSnapshotNode:
    def test_simple_button(self):
        node = {"role": "button", "name": "Submit", "ref": "e1"}
        assert _render_snapshot_node(node) == '- button "Submit" [ref=e1]\n'

    def test_text_node(self):
        node = {"role": "text", "content": "Hello world"}
        assert _render_snapshot_node(node) == "- text: Hello world\n"

    def test_empty_text_node(self):
        node = {"role": "text", "content": ""}
        assert _render_snapshot_node(node) == ""

    def test_heading_with_level(self):
        node = {"role": "heading", "name": "Title", "ref": "e1", "level": 2}
        assert _render_snapshot_node(node) == '- heading "Title" [ref=e1] [level=2]\n'

    def test_checkbox_with_checked(self):
        node = {"role": "checkbox", "name": "Accept", "ref": "e1", "checked": True}
        assert _render_snapshot_node(node) == '- checkbox "Accept" [ref=e1] [checked=true]\n'

    def test_textbox_with_value(self):
        node = {"role": "textbox", "name": "Email", "ref": "e1", "value": "test@x.com"}
        assert _render_snapshot_node(node) == '- textbox "Email" [ref=e1] [value="test@x.com"]\n'

    def test_empty_value_not_shown(self):
        node = {"role": "textbox", "name": "Search", "ref": "e1", "value": ""}
        assert _render_snapshot_node(node) == '- textbox "Search" [ref=e1]\n'

    def test_link_with_url(self):
        node = {"role": "link", "ref": "e1", "url": "https://example.com",
                "children": [{"role": "text", "content": "Example"}]}
        output = _render_snapshot_node(node)
        assert "- link [ref=e1]:" in output
        assert "- /url: https://example.com" in output
        assert "- text: Example" in output

    def test_nested_tree(self):
        tree = {
            "role": "navigation",
            "children": [
                {"role": "list", "children": [
                    {"role": "listitem", "children": [
                        {"role": "link", "name": "Home", "ref": "e1"}
                    ]},
                ]},
            ],
        }
        output = _render_snapshot_node(tree)
        assert "- navigation:" in output
        assert '  - list:' in output
        assert '    - listitem:' in output
        assert '      - link "Home" [ref=e1]' in output

    def test_depth_indentation(self):
        node = {"role": "button", "name": "Deep", "ref": "e5"}
        output = _render_snapshot_node(node, depth=3)
        assert output == '      - button "Deep" [ref=e5]\n'

    def test_no_ref_no_name(self):
        node = {"role": "generic"}
        assert _render_snapshot_node(node) == "- generic\n"

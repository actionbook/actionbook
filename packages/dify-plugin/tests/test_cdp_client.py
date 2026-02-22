"""Tests for utils/cdp_client.py."""

import sys
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent))

from utils.cdp_client import CdpConnectionError, _get_active_page, cdp_page, validate_cdp_url


# ---------------------------------------------------------------------------
# validate_cdp_url
# ---------------------------------------------------------------------------


class TestValidateCdpUrl:
    @pytest.mark.parametrize("url", [
        "ws://localhost:9222",
        "wss://production-sfo.browserless.io?token=TOKEN",
        "http://localhost:9222",
        "https://cloud-browser.example.com",
    ])
    def test_valid_urls_returned_stripped(self, url):
        assert validate_cdp_url(f"  {url}  ") == url

    @pytest.mark.parametrize("bad", ["", "   ", None])
    def test_empty_raises(self, bad):
        with pytest.raises(ValueError, match="required"):
            validate_cdp_url(bad)

    @pytest.mark.parametrize("bad", [
        "ftp://host",
        "localhost:9222",
        "//host:9222",
        "random-string",
    ])
    def test_invalid_prefix_raises(self, bad):
        with pytest.raises(ValueError, match="Invalid cdp_url"):
            validate_cdp_url(bad)

    def test_insecure_non_localhost_warns(self, caplog):
        """ws:// to a remote host should log a warning."""
        import logging
        with caplog.at_level(logging.WARNING, logger="utils.cdp_client"):
            result = validate_cdp_url("ws://remote-host:9222")
        assert result == "ws://remote-host:9222"
        assert "Insecure CDP connection" in caplog.text

    @pytest.mark.parametrize("url", [
        "ws://localhost:9222",
        "ws://127.0.0.1:9222",
        "http://localhost:9222",
    ])
    def test_insecure_localhost_no_warning(self, url, caplog):
        """ws:// to localhost should not log a warning."""
        import logging
        with caplog.at_level(logging.WARNING, logger="utils.cdp_client"):
            validate_cdp_url(url)
        assert "Insecure CDP connection" not in caplog.text

    def test_secure_remote_no_warning(self, caplog):
        """wss:// to remote host should not log a warning."""
        import logging
        with caplog.at_level(logging.WARNING, logger="utils.cdp_client"):
            validate_cdp_url("wss://remote-host:9222")
        assert "Insecure CDP connection" not in caplog.text


# ---------------------------------------------------------------------------
# _get_active_page
# ---------------------------------------------------------------------------


class TestGetActivePage:
    def test_returns_first_page_if_exists(self):
        mock_page = MagicMock()
        mock_context = MagicMock()
        mock_context.pages = [mock_page]
        mock_browser = MagicMock()
        mock_browser.contexts = [mock_context]

        result = _get_active_page(mock_browser)
        assert result is mock_page

    def test_creates_new_page_when_context_has_no_pages(self):
        new_page = MagicMock()
        mock_context = MagicMock()
        mock_context.pages = []
        mock_context.new_page.return_value = new_page
        mock_browser = MagicMock()
        mock_browser.contexts = [mock_context]

        result = _get_active_page(mock_browser)
        assert result is new_page
        mock_context.new_page.assert_called_once()

    def test_creates_new_context_and_page_when_no_contexts(self):
        new_page = MagicMock()
        new_context = MagicMock()
        new_context.new_page.return_value = new_page
        mock_browser = MagicMock()
        mock_browser.contexts = []
        mock_browser.new_context.return_value = new_context

        result = _get_active_page(mock_browser)
        assert result is new_page
        mock_browser.new_context.assert_called_once()
        new_context.new_page.assert_called_once()


# ---------------------------------------------------------------------------
# cdp_page context manager
# ---------------------------------------------------------------------------


class TestCdpPage:
    def _make_mock_page(self):
        page = MagicMock()
        context = MagicMock()
        context.pages = [page]
        browser = MagicMock()
        browser.contexts = [context]
        return browser, page

    @patch("utils.cdp_client.sync_playwright")
    def test_yields_page_on_success(self, mock_playwright):
        browser, page = self._make_mock_page()
        mock_p = MagicMock()
        mock_p.chromium.connect_over_cdp.return_value = browser
        mock_playwright.return_value.__enter__ = MagicMock(return_value=mock_p)
        mock_playwright.return_value.__exit__ = MagicMock(return_value=False)

        with cdp_page("ws://localhost:9222") as p:
            assert p is page

    @patch("utils.cdp_client.sync_playwright")
    def test_closes_browser_after_yield(self, mock_playwright):
        browser, _ = self._make_mock_page()
        mock_p = MagicMock()
        mock_p.chromium.connect_over_cdp.return_value = browser
        mock_playwright.return_value.__enter__ = MagicMock(return_value=mock_p)
        mock_playwright.return_value.__exit__ = MagicMock(return_value=False)

        with cdp_page("ws://localhost:9222"):
            pass

        browser.close.assert_called_once()

    @patch("utils.cdp_client.sync_playwright")
    def test_raises_cdp_connection_error_on_connect_failure(self, mock_playwright):
        mock_p = MagicMock()
        mock_p.chromium.connect_over_cdp.side_effect = Exception("connection refused")
        mock_playwright.return_value.__enter__ = MagicMock(return_value=mock_p)
        mock_playwright.return_value.__exit__ = MagicMock(return_value=False)

        with pytest.raises(CdpConnectionError, match="Cannot connect"):
            with cdp_page("ws://localhost:9222"):
                pass

    def test_raises_for_invalid_url(self):
        with pytest.raises(ValueError, match="Invalid cdp_url"):
            with cdp_page("not-a-url"):
                pass

    def test_raises_for_empty_url(self):
        with pytest.raises(ValueError, match="required"):
            with cdp_page(""):
                pass

    @patch("utils.cdp_client.sync_playwright")
    def test_closes_browser_even_when_page_raises(self, mock_playwright):
        browser, page = self._make_mock_page()
        mock_p = MagicMock()
        mock_p.chromium.connect_over_cdp.return_value = browser
        mock_playwright.return_value.__enter__ = MagicMock(return_value=mock_p)
        mock_playwright.return_value.__exit__ = MagicMock(return_value=False)

        with pytest.raises(RuntimeError):
            with cdp_page("ws://localhost:9222") as p:
                raise RuntimeError("page broke")

        browser.close.assert_called_once()

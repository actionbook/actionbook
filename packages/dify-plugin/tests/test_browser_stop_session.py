"""Tests for BrowserStopSessionTool."""

import sys
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent))

from tools.browser_stop_session import BrowserStopSessionTool


def _make_tool() -> BrowserStopSessionTool:
    return BrowserStopSessionTool.from_credentials({})


class TestBrowserStopSessionTool:
    def setup_method(self):
        self.tool = _make_tool()

    @patch("tools.browser_stop_session.pool")
    @patch("tools.browser_stop_session.get_provider")
    def test_success(self, mock_get_provider, mock_pool):
        """Test successful session stop using cached provider info."""
        mock_pool.get_session_info.return_value = ("hyperbrowser", "hb-test-key")
        mock_provider = MagicMock()
        mock_get_provider.return_value = mock_provider

        result = list(self.tool._invoke({
            "session_id": "s-abc",
        }))

        assert len(result) == 1
        text = result[0].message.text
        assert "stopped" in text.lower()
        assert "s-abc" in text
        mock_provider.stop_session.assert_called_once_with("s-abc")
        mock_pool.disconnect.assert_called_once_with("s-abc")

    def test_missing_session_id_returns_error(self):
        """Test error when session_id is missing."""
        result = list(self.tool._invoke({}))
        assert len(result) == 1
        assert "Error" in result[0].message.text
        assert "session_id" in result[0].message.text

    @patch("tools.browser_stop_session.pool")
    def test_unknown_session_id_returns_error(self, mock_pool):
        """Test error when session_id is not found in pool."""
        mock_pool.get_session_info.return_value = None

        result = list(self.tool._invoke({
            "session_id": "s-unknown",
        }))
        assert len(result) == 1
        assert "Error" in result[0].message.text
        assert "s-unknown" in result[0].message.text

    @patch("tools.browser_stop_session.pool")
    @patch("tools.browser_stop_session.get_provider")
    def test_provider_exception_returns_error(self, mock_get_provider, mock_pool):
        """Test error when provider.stop_session raises."""
        mock_pool.get_session_info.return_value = ("hyperbrowser", "hb-test-key")
        mock_provider = MagicMock()
        mock_provider.stop_session.side_effect = RuntimeError("network failure")
        mock_get_provider.return_value = mock_provider

        result = list(self.tool._invoke({
            "session_id": "s-abc",
        }))

        assert len(result) == 1
        assert "Error" in result[0].message.text
        mock_pool.disconnect.assert_not_called()

    @patch("tools.browser_stop_session.pool")
    @patch("tools.browser_stop_session.get_provider")
    def test_disconnect_happens_after_remote_stop(self, mock_get_provider, mock_pool):
        mock_pool.get_session_info.return_value = ("hyperbrowser", "hb-test-key")
        mock_provider = MagicMock()

        def _stop_side_effect(_session_id):
            mock_pool.disconnect.assert_not_called()

        mock_provider.stop_session.side_effect = _stop_side_effect
        mock_get_provider.return_value = mock_provider

        list(self.tool._invoke({
            "session_id": "s-abc",
        }))

        mock_provider.stop_session.assert_called_once_with("s-abc")
        mock_pool.disconnect.assert_called_once_with("s-abc")

    @patch("tools.browser_stop_session.pool")
    @patch("tools.browser_stop_session.get_provider")
    def test_uses_cached_provider_and_key(self, mock_get_provider, mock_pool):
        """Test that provider name and api_key come from pool cache."""
        mock_pool.get_session_info.return_value = ("steel", "steel-key-123")
        mock_provider = MagicMock()
        mock_get_provider.return_value = mock_provider

        list(self.tool._invoke({"session_id": "s-xyz"}))

        mock_get_provider.assert_called_once_with("steel", "steel-key-123")

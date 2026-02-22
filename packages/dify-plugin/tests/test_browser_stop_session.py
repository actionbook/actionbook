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

    @patch("tools.browser_stop_session.get_provider")
    def test_success(self, mock_get_provider):
        """Test successful session stop."""
        mock_provider = MagicMock()
        mock_get_provider.return_value = mock_provider

        result = list(self.tool._invoke({
            "provider": "hyperbrowser",
            "api_key": "hb-test-key",
            "session_id": "s-abc",
        }))

        assert len(result) == 1
        text = result[0].message.text
        assert "stopped" in text.lower()
        assert "s-abc" in text
        mock_provider.stop_session.assert_called_once_with("s-abc")

    def test_missing_api_key_returns_error(self):
        """Test error when api_key is missing."""
        result = list(self.tool._invoke({
            "provider": "hyperbrowser",
            "session_id": "s-abc",
        }))
        assert len(result) == 1
        assert "Error" in result[0].message.text
        assert "api_key" in result[0].message.text

    def test_missing_session_id_returns_error(self):
        """Test error when session_id is missing."""
        result = list(self.tool._invoke({
            "provider": "hyperbrowser",
            "api_key": "hb-test-key",
        }))
        assert len(result) == 1
        assert "Error" in result[0].message.text
        assert "session_id" in result[0].message.text

    def test_unknown_provider_returns_error(self):
        """Test error for unknown provider name."""
        result = list(self.tool._invoke({
            "provider": "nonexistent",
            "api_key": "key",
            "session_id": "s-1",
        }))
        assert len(result) == 1
        assert "Error" in result[0].message.text

    @patch("tools.browser_stop_session.get_provider")
    def test_provider_exception_returns_error(self, mock_get_provider):
        """Test error when provider.stop_session raises."""
        mock_provider = MagicMock()
        mock_provider.stop_session.side_effect = RuntimeError("network failure")
        mock_get_provider.return_value = mock_provider

        result = list(self.tool._invoke({
            "provider": "hyperbrowser",
            "api_key": "key",
            "session_id": "s-abc",
        }))

        assert len(result) == 1
        assert "Error" in result[0].message.text
        assert "RuntimeError" in result[0].message.text

    def test_not_implemented_provider_returns_error(self):
        """Test error for a registered but unimplemented provider."""
        result = list(self.tool._invoke({
            "provider": "steel",
            "api_key": "key",
            "session_id": "s-1",
        }))
        assert len(result) == 1
        assert "Error" in result[0].message.text
        assert "not yet implemented" in result[0].message.text.lower()

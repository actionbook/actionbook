"""Tests for BrowserCreateSessionTool."""

import json
import sys
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent))

from tools.browser_create_session import BrowserCreateSessionTool


def _make_tool() -> BrowserCreateSessionTool:
    return BrowserCreateSessionTool.from_credentials({})


class TestBrowserCreateSessionTool:
    def setup_method(self):
        self.tool = _make_tool()

    def _fake_session(self, ws_endpoint="wss://example.com/s/abc", session_id="s-abc"):
        session = MagicMock()
        session.ws_endpoint = ws_endpoint
        session.session_id = session_id
        return session

    @patch("tools.browser_create_session.get_provider")
    def test_success_returns_ws_endpoint_and_session_id(self, mock_get_provider):
        fake_session = self._fake_session()
        mock_provider = MagicMock()
        mock_provider.create_session.return_value = fake_session
        mock_get_provider.return_value = mock_provider

        result = list(self.tool._invoke({
            "provider": "hyperbrowser",
            "api_key": "hb-test-key",
        }))

        assert len(result) == 1
        text = result[0].message.text
        assert "wss://example.com/s/abc" in text
        assert "s-abc" in text
        assert "hyperbrowser" in text

    @patch("tools.browser_create_session.get_provider")
    def test_json_block_in_output(self, mock_get_provider):
        fake_session = self._fake_session()
        mock_provider = MagicMock()
        mock_provider.create_session.return_value = fake_session
        mock_get_provider.return_value = mock_provider

        result = list(self.tool._invoke({
            "provider": "hyperbrowser",
            "api_key": "test-key",
        }))

        text = result[0].message.text
        # Extract JSON block
        json_str = text.split("```json\n")[1].split("\n```")[0]
        data = json.loads(json_str)
        assert data["ws_endpoint"] == "wss://example.com/s/abc"
        assert data["session_id"] == "s-abc"
        assert data["provider"] == "hyperbrowser"

    @patch("tools.browser_create_session.get_provider")
    def test_passes_profile_id_to_provider(self, mock_get_provider):
        mock_provider = MagicMock()
        mock_provider.create_session.return_value = self._fake_session()
        mock_get_provider.return_value = mock_provider

        list(self.tool._invoke({
            "provider": "hyperbrowser",
            "api_key": "key",
            "profile_id": "user-42",
        }))

        mock_provider.create_session.assert_called_once_with(
            profile_id="user-42",
            use_proxy=False,
        )

    @patch("tools.browser_create_session.get_provider")
    def test_empty_profile_id_passed_as_none(self, mock_get_provider):
        mock_provider = MagicMock()
        mock_provider.create_session.return_value = self._fake_session()
        mock_get_provider.return_value = mock_provider

        list(self.tool._invoke({
            "provider": "hyperbrowser",
            "api_key": "key",
            "profile_id": "   ",  # whitespace only → None
        }))

        mock_provider.create_session.assert_called_once_with(
            profile_id=None,
            use_proxy=False,
        )

    @patch("tools.browser_create_session.get_provider")
    def test_use_proxy_true(self, mock_get_provider):
        mock_provider = MagicMock()
        mock_provider.create_session.return_value = self._fake_session()
        mock_get_provider.return_value = mock_provider

        list(self.tool._invoke({
            "provider": "hyperbrowser",
            "api_key": "key",
            "use_proxy": "true",
        }))

        _, kwargs = mock_provider.create_session.call_args
        assert kwargs["use_proxy"] is True

    def test_missing_api_key_returns_error(self):
        result = list(self.tool._invoke({"provider": "hyperbrowser"}))
        assert len(result) == 1
        assert "Error" in result[0].message.text
        assert "api_key" in result[0].message.text

    def test_empty_api_key_returns_error(self):
        result = list(self.tool._invoke({"provider": "hyperbrowser", "api_key": "  "}))
        assert "Error" in result[0].message.text

    def test_unknown_provider_returns_error(self):
        result = list(self.tool._invoke({
            "provider": "nonexistent",
            "api_key": "key",
        }))
        assert "Error" in result[0].message.text

    def test_not_implemented_provider_returns_error(self):
        result = list(self.tool._invoke({
            "provider": "steel",
            "api_key": "key",
        }))
        assert "Error" in result[0].message.text
        assert "not yet implemented" in result[0].message.text.lower()

    @patch("tools.browser_create_session.get_provider")
    def test_provider_exception_returns_error(self, mock_get_provider):
        mock_provider = MagicMock()
        mock_provider.create_session.side_effect = RuntimeError("network failure")
        mock_get_provider.return_value = mock_provider

        result = list(self.tool._invoke({
            "provider": "hyperbrowser",
            "api_key": "key",
        }))

        assert "Error" in result[0].message.text
        assert "RuntimeError" in result[0].message.text

    @patch("tools.browser_create_session.get_provider")
    def test_use_proxy_boolean_true(self, mock_get_provider):
        """Test that boolean True for use_proxy is handled correctly."""
        mock_provider = MagicMock()
        mock_provider.create_session.return_value = self._fake_session()
        mock_get_provider.return_value = mock_provider

        list(self.tool._invoke({
            "api_key": "key",
            "use_proxy": True,
        }))

        _, kwargs = mock_provider.create_session.call_args
        assert kwargs["use_proxy"] is True

    @patch("tools.browser_create_session.get_provider")
    def test_use_proxy_string_TRUE_uppercase(self, mock_get_provider):
        """Test that 'TRUE' (uppercase) is handled correctly."""
        mock_provider = MagicMock()
        mock_provider.create_session.return_value = self._fake_session()
        mock_get_provider.return_value = mock_provider

        list(self.tool._invoke({
            "api_key": "key",
            "use_proxy": "TRUE",
        }))

        _, kwargs = mock_provider.create_session.call_args
        assert kwargs["use_proxy"] is True

    @patch("tools.browser_create_session.get_provider")
    def test_use_proxy_string_with_whitespace(self, mock_get_provider):
        """Test that ' true ' (with whitespace) is handled correctly."""
        mock_provider = MagicMock()
        mock_provider.create_session.return_value = self._fake_session()
        mock_get_provider.return_value = mock_provider

        list(self.tool._invoke({
            "api_key": "key",
            "use_proxy": " true ",
        }))

        _, kwargs = mock_provider.create_session.call_args
        assert kwargs["use_proxy"] is True

    @patch("tools.browser_create_session.get_provider")
    def test_use_proxy_false_string(self, mock_get_provider):
        """Test that 'false' string correctly resolves to False."""
        mock_provider = MagicMock()
        mock_provider.create_session.return_value = self._fake_session()
        mock_get_provider.return_value = mock_provider

        list(self.tool._invoke({
            "api_key": "key",
            "use_proxy": "false",
        }))

        _, kwargs = mock_provider.create_session.call_args
        assert kwargs["use_proxy"] is False

    @patch("tools.browser_create_session.get_provider")
    def test_default_provider_is_hyperbrowser(self, mock_get_provider):
        """Test that omitting provider defaults to hyperbrowser."""
        mock_provider = MagicMock()
        mock_provider.create_session.return_value = self._fake_session()
        mock_get_provider.return_value = mock_provider

        list(self.tool._invoke({"api_key": "key"}))

        mock_get_provider.assert_called_once_with("hyperbrowser", "key")

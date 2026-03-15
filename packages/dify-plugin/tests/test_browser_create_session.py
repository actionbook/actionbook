"""Tests for BrowserCreateSessionTool."""

import json
import sys
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent))

from tools.browser_create_session import BrowserCreateSessionTool


def _make_tool(hyperbrowser_key: str = "hb-test-key") -> BrowserCreateSessionTool:
    return BrowserCreateSessionTool.from_credentials(
        {"hyperbrowser_api_key": hyperbrowser_key}
    )


class TestBrowserCreateSessionTool:
    def setup_method(self):
        self.tool = _make_tool()

    def _fake_session(self, ws_endpoint="wss://example.com/s/abc", session_id="s-abc"):
        session = MagicMock()
        session.ws_endpoint = ws_endpoint
        session.session_id = session_id
        return session

    @patch("tools.browser_create_session.pool")
    @patch("tools.browser_create_session.get_provider")
    def test_json_block_in_output(self, mock_get_provider, _mock_pool):
        fake_session = self._fake_session()
        mock_provider = MagicMock()
        mock_provider.create_session.return_value = fake_session
        mock_get_provider.return_value = mock_provider

        result = list(self.tool._invoke({
            "provider": "hyperbrowser",
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
            "profile_id": "   ",  # whitespace only → None
        }))

        mock_provider.create_session.assert_called_once_with(
            profile_id=None,
            use_proxy=False,
        )

    @pytest.mark.parametrize("use_proxy_value", ["true", True, "TRUE", " true "])
    @patch("tools.browser_create_session.get_provider")
    def test_use_proxy_truthy_values(self, mock_get_provider, use_proxy_value):
        """Test that various truthy use_proxy inputs resolve to True."""
        mock_provider = MagicMock()
        mock_provider.create_session.return_value = self._fake_session()
        mock_get_provider.return_value = mock_provider

        list(self.tool._invoke({
            "provider": "hyperbrowser",
            "use_proxy": use_proxy_value,
        }))

        _, kwargs = mock_provider.create_session.call_args
        assert kwargs["use_proxy"] is True

    def test_missing_api_key_returns_error(self):
        """Test that missing hyperbrowser_api_key in credentials returns error."""
        tool = _make_tool(hyperbrowser_key="")
        result = list(tool._invoke({"provider": "hyperbrowser"}))
        assert len(result) == 1
        assert "Error" in result[0].message.text
        assert "Hyperbrowser API Key" in result[0].message.text

    def test_unknown_provider_returns_error(self):
        result = list(self.tool._invoke({
            "provider": "nonexistent",
        }))
        assert "Error" in result[0].message.text

    def test_not_implemented_provider_returns_error(self):
        result = list(self.tool._invoke({
            "provider": "steel",
        }))
        assert "Error" in result[0].message.text
        # Steel is still in SUPPORTED_PROVIDERS but raises NotImplementedError
        assert "not yet implemented" in result[0].message.text.lower() or "Error" in result[0].message.text

    @patch("tools.browser_create_session.get_provider")
    def test_provider_exception_returns_error(self, mock_get_provider):
        mock_provider = MagicMock()
        mock_provider.create_session.side_effect = RuntimeError("network failure")
        mock_get_provider.return_value = mock_provider

        result = list(self.tool._invoke({
            "provider": "hyperbrowser",
        }))

        assert "Error" in result[0].message.text

    @patch("tools.browser_create_session.get_provider")
    def test_use_proxy_false_string(self, mock_get_provider):
        """Test that 'false' string correctly resolves to False."""
        mock_provider = MagicMock()
        mock_provider.create_session.return_value = self._fake_session()
        mock_get_provider.return_value = mock_provider

        list(self.tool._invoke({
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

        list(self.tool._invoke({}))

        mock_get_provider.assert_called_once_with("hyperbrowser", "hb-test-key")

    @patch("tools.browser_create_session.pool")
    @patch("tools.browser_create_session.get_provider")
    def test_pool_connect_called_after_create(self, mock_get_provider, mock_pool):
        """After creating a session, pool.connect should be called."""
        fake_session = self._fake_session()
        mock_provider = MagicMock()
        mock_provider.create_session.return_value = fake_session
        mock_get_provider.return_value = mock_provider

        result = list(self.tool._invoke({}))

        mock_pool.connect.assert_called_once_with(
            "s-abc", "wss://example.com/s/abc",
            provider_name="hyperbrowser", api_key="hb-test-key",
        )
        assert "Error" not in result[0].message.text

    @patch("tools.browser_create_session.pool")
    @patch("tools.browser_create_session.get_provider")
    def test_pool_connect_failure_returns_error_and_rolls_back_remote_session(
        self, mock_get_provider, mock_pool,
    ):
        """Pool connect failure should fail fast and stop the remote session."""
        fake_session = self._fake_session()
        mock_provider = MagicMock()
        mock_provider.create_session.return_value = fake_session
        mock_get_provider.return_value = mock_provider
        mock_pool.connect.side_effect = RuntimeError("pool error")

        result = list(self.tool._invoke({}))

        assert "Error" in result[0].message.text
        assert "pool error" in result[0].message.text
        assert "closed automatically" in result[0].message.text
        mock_provider.stop_session.assert_called_once_with("s-abc")

    @patch("tools.browser_create_session.pool")
    @patch("tools.browser_create_session.get_provider")
    def test_pool_connect_failure_and_cleanup_failure_returns_recovery_hint(
        self, mock_get_provider, mock_pool,
    ):
        fake_session = self._fake_session()
        mock_provider = MagicMock()
        mock_provider.create_session.return_value = fake_session
        mock_provider.stop_session.side_effect = RuntimeError("provider stop failed")
        mock_get_provider.return_value = mock_provider
        mock_pool.connect.side_effect = RuntimeError("pool error")

        result = list(self.tool._invoke({}))

        text = result[0].message.text
        assert "Error" in text
        assert "Automatic remote cleanup also failed" in text
        assert "s-abc" in text

"""Tests for browser provider abstraction."""

import sys
import uuid as _uuid
from pathlib import Path
from unittest.mock import MagicMock, patch, Mock

import pytest
import requests

sys.path.insert(0, str(Path(__file__).parent.parent))

from providers import SUPPORTED_PROVIDERS, get_provider
from providers.base import BrowserProvider, BrowserSession
from providers.hyperbrowser import HyperbrowserProvider, HyperbrowserSession
from providers.steel import SteelProvider


# ---------------------------------------------------------------------------
# get_provider factory
# ---------------------------------------------------------------------------


class TestGetProvider:
    def test_returns_hyperbrowser_provider(self):
        provider = get_provider("hyperbrowser", "test-key")
        assert isinstance(provider, HyperbrowserProvider)

    def test_raises_for_unknown_provider(self):
        with pytest.raises(ValueError, match="Unknown provider"):
            get_provider("nonexistent", "key")

    def test_supported_providers_list(self):
        assert "hyperbrowser" in SUPPORTED_PROVIDERS
        assert "steel" in SUPPORTED_PROVIDERS


# ---------------------------------------------------------------------------
# HyperbrowserProvider (HTTP API)
# ---------------------------------------------------------------------------


class TestHyperbrowserProvider:
    def test_init_stores_api_key(self):
        provider = HyperbrowserProvider(api_key="test-key")
        assert provider._api_key == "test-key"

    @patch("providers.hyperbrowser.requests.post")
    def test_create_session_returns_session(self, mock_post):
        mock_resp = Mock()
        mock_resp.status_code = 200
        mock_resp.json.return_value = {
            "id": "session-abc-123",
            "wsEndpoint": "wss://example.com/session/abc",
        }
        mock_resp.raise_for_status = Mock()
        mock_post.return_value = mock_resp

        provider = HyperbrowserProvider(api_key="key")
        session = provider.create_session()

        assert session.ws_endpoint == "wss://example.com/session/abc"
        assert session.session_id == "session-abc-123"

    @patch("providers.hyperbrowser.requests.post")
    def test_create_session_with_profile_id(self, mock_post):
        mock_resp = Mock()
        mock_resp.status_code = 200
        mock_resp.json.return_value = {
            "id": "session-xyz",
            "wsEndpoint": "wss://example.com/s/xyz",
        }
        mock_resp.raise_for_status = Mock()
        mock_post.return_value = mock_resp

        provider = HyperbrowserProvider(api_key="key")
        provider.create_session(profile_id="user-42", use_proxy=True)

        _, kwargs = mock_post.call_args
        body = kwargs["json"]
        expected_uuid = str(_uuid.uuid5(_uuid.NAMESPACE_URL, "actionbook:user-42"))
        assert body["profile"] == {
            "id": expected_uuid,
            "persistChanges": True,
        }
        assert body["useProxy"] is True

    @patch("providers.hyperbrowser.requests.post")
    def test_create_session_no_profile_when_profile_id_is_none(self, mock_post):
        mock_resp = Mock()
        mock_resp.status_code = 200
        mock_resp.json.return_value = {
            "id": "s1",
            "wsEndpoint": "wss://x",
        }
        mock_resp.raise_for_status = Mock()
        mock_post.return_value = mock_resp

        provider = HyperbrowserProvider(api_key="key")
        provider.create_session(profile_id=None)

        _, kwargs = mock_post.call_args
        body = kwargs["json"]
        assert "profile" not in body

    @patch("providers.hyperbrowser.requests.put")
    def test_stop_session_calls_api(self, mock_put):
        mock_resp = Mock()
        mock_resp.raise_for_status = Mock()
        mock_put.return_value = mock_resp

        provider = HyperbrowserProvider(api_key="key")
        provider.stop_session("session-xyz")

        mock_put.assert_called_once()
        args, kwargs = mock_put.call_args
        assert "session-xyz/stop" in args[0]
        assert kwargs["headers"]["x-api-key"] == "key"

    @patch("providers.hyperbrowser.requests.post")
    def test_create_session_incomplete_data_raises(self, mock_post):
        mock_resp = Mock()
        mock_resp.status_code = 200
        mock_resp.json.return_value = {"id": "s1"}  # missing wsEndpoint
        mock_resp.raise_for_status = Mock()
        mock_post.return_value = mock_resp

        provider = HyperbrowserProvider(api_key="key")
        with pytest.raises(RuntimeError, match="incomplete session data"):
            provider.create_session()

    @patch("providers.hyperbrowser.requests.post")
    def test_create_session_http_error_raises(self, mock_post):
        mock_resp = Mock()
        mock_resp.ok = False
        mock_resp.status_code = 403
        mock_resp.text = "Forbidden"
        mock_resp.url = "https://api.hyperbrowser.ai/api/session"
        mock_post.return_value = mock_resp

        provider = HyperbrowserProvider(api_key="key")
        with pytest.raises(RuntimeError, match="HTTP 403"):
            provider.create_session()


# ---------------------------------------------------------------------------
# HyperbrowserSession
# ---------------------------------------------------------------------------


class TestHyperbrowserSession:
    def _make_session(self) -> HyperbrowserSession:
        return HyperbrowserSession(
            _ws_endpoint="wss://example.com/session/abc",
            _session_id="session-abc",
            _api_key="test-key",
        )

    def test_ws_endpoint_property(self):
        s = self._make_session()
        assert s.ws_endpoint == "wss://example.com/session/abc"

    def test_session_id_property(self):
        s = self._make_session()
        assert s.session_id == "session-abc"

    @patch("providers.hyperbrowser.requests.put")
    def test_stop_calls_api(self, mock_put):
        mock_resp = Mock()
        mock_resp.raise_for_status = Mock()
        mock_put.return_value = mock_resp

        s = HyperbrowserSession(
            _ws_endpoint="wss://x", _session_id="s-1", _api_key="key"
        )
        s.stop()

        mock_put.assert_called_once()
        args, _ = mock_put.call_args
        assert "s-1/stop" in args[0]

    @patch("providers.hyperbrowser.requests.put")
    def test_stop_re_raises_on_api_error(self, mock_put):
        mock_resp = Mock()
        mock_resp.raise_for_status.side_effect = Exception("network error")
        mock_put.return_value = mock_resp

        s = HyperbrowserSession(
            _ws_endpoint="wss://x", _session_id="s-1", _api_key="key"
        )
        with pytest.raises(Exception, match="network error"):
            s.stop()

    def test_satisfies_browser_session_protocol(self):
        s = self._make_session()
        assert isinstance(s, BrowserSession)


# ---------------------------------------------------------------------------
# SteelProvider (stub)
# ---------------------------------------------------------------------------


class TestSteelProvider:
    def test_init_raises_not_implemented(self):
        with pytest.raises(NotImplementedError, match="Steel.dev provider is not yet implemented"):
            SteelProvider(api_key="key")

    def test_create_session_raises_not_implemented(self):
        with pytest.raises(NotImplementedError):
            SteelProvider.create_session(None)  # type: ignore[arg-type]

    def test_stop_session_raises_not_implemented(self):
        with pytest.raises(NotImplementedError):
            SteelProvider.stop_session(None, "s-id")  # type: ignore[arg-type]

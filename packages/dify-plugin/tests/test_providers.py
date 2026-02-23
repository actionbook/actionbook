"""Tests for browser provider abstraction."""

import sys
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

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
        with patch("providers.hyperbrowser.HyperbrowserProvider.__init__", return_value=None):
            provider = get_provider("hyperbrowser", "test-key")
        assert isinstance(provider, HyperbrowserProvider)

    def test_raises_for_unknown_provider(self):
        with pytest.raises(ValueError, match="Unknown provider"):
            get_provider("nonexistent", "key")

    def test_supported_providers_list(self):
        assert "hyperbrowser" in SUPPORTED_PROVIDERS
        assert "steel" in SUPPORTED_PROVIDERS


# ---------------------------------------------------------------------------
# HyperbrowserProvider
# ---------------------------------------------------------------------------


class TestHyperbrowserProvider:
    def _make_provider(self, api_key: str = "hb-test-key") -> HyperbrowserProvider:
        with patch("providers.hyperbrowser.Hyperbrowser") as MockHB:
            MockHB.return_value = MagicMock()
            provider = HyperbrowserProvider(api_key=api_key)
        return provider

    def test_init_creates_client(self):
        with patch("providers.hyperbrowser.Hyperbrowser") as MockHB:
            HyperbrowserProvider(api_key="test-key")
        MockHB.assert_called_once_with(api_key="test-key")

    def test_create_session_returns_session(self):
        with patch("providers.hyperbrowser.Hyperbrowser") as MockHB:
            mock_client = MagicMock()
            MockHB.return_value = mock_client

            fake_session = MagicMock()
            fake_session.ws_endpoint = "wss://example.com/session/abc"
            fake_session.id = "session-abc-123"
            mock_client.sessions.create.return_value = fake_session

            with patch("providers.hyperbrowser.CreateSessionParams") as MockParams:
                MockParams.return_value = MagicMock()
                provider = HyperbrowserProvider(api_key="key")
                session = provider.create_session()

        assert session.ws_endpoint == "wss://example.com/session/abc"
        assert session.session_id == "session-abc-123"

    def test_create_session_with_profile_id(self):
        import uuid as _uuid

        with patch("providers.hyperbrowser.Hyperbrowser") as MockHB:
            mock_client = MagicMock()
            MockHB.return_value = mock_client

            fake_session = MagicMock()
            fake_session.ws_endpoint = "wss://example.com/s/xyz"
            fake_session.id = "session-xyz"
            mock_client.sessions.create.return_value = fake_session

            with patch("providers.hyperbrowser.CreateSessionParams") as MockParams:
                captured_kwargs = {}

                def capture(**kwargs):
                    captured_kwargs.update(kwargs)
                    return MagicMock()

                MockParams.side_effect = capture
                provider = HyperbrowserProvider(api_key="key")
                provider.create_session(profile_id="user-42", use_proxy=True)

        # profile_id is normalized to UUID5 for Hyperbrowser API compatibility
        expected_uuid = str(_uuid.uuid5(_uuid.NAMESPACE_URL, "actionbook:user-42"))
        assert captured_kwargs.get("profile") == {
            "id": expected_uuid,
            "persist_changes": True,
        }
        assert captured_kwargs.get("use_proxy") is True

    def test_create_session_no_profile_when_profile_id_is_none(self):
        with patch("providers.hyperbrowser.Hyperbrowser") as MockHB:
            mock_client = MagicMock()
            MockHB.return_value = mock_client
            mock_client.sessions.create.return_value = MagicMock(
                ws_endpoint="wss://x", id="s1"
            )

            with patch("providers.hyperbrowser.CreateSessionParams") as MockParams:
                captured_kwargs = {}

                def capture(**kwargs):
                    captured_kwargs.update(kwargs)
                    return MagicMock()

                MockParams.side_effect = capture
                provider = HyperbrowserProvider(api_key="key")
                provider.create_session(profile_id=None)

        assert "profile" not in captured_kwargs

    def test_stop_session_calls_client(self):
        with patch("providers.hyperbrowser.Hyperbrowser") as MockHB:
            mock_client = MagicMock()
            MockHB.return_value = mock_client
            provider = HyperbrowserProvider(api_key="key")
            provider.stop_session("session-xyz")

        mock_client.sessions.stop.assert_called_once_with("session-xyz")

    def test_import_error_when_package_missing(self):
        with patch("providers.hyperbrowser.Hyperbrowser", None):
            with pytest.raises(ImportError, match="hyperbrowser package is not installed"):
                HyperbrowserProvider(api_key="key")


# ---------------------------------------------------------------------------
# HyperbrowserSession
# ---------------------------------------------------------------------------


class TestHyperbrowserSession:
    def _make_session(self) -> HyperbrowserSession:
        mock_client = MagicMock()
        return HyperbrowserSession(
            _ws_endpoint="wss://example.com/session/abc",
            _session_id="session-abc",
            _client=mock_client,
        )

    def test_ws_endpoint_property(self):
        s = self._make_session()
        assert s.ws_endpoint == "wss://example.com/session/abc"

    def test_session_id_property(self):
        s = self._make_session()
        assert s.session_id == "session-abc"

    def test_stop_calls_client(self):
        mock_client = MagicMock()
        s = HyperbrowserSession(
            _ws_endpoint="wss://x", _session_id="s-1", _client=mock_client
        )
        s.stop()
        mock_client.sessions.stop.assert_called_once_with("s-1")

    def test_stop_re_raises_on_client_error(self):
        mock_client = MagicMock()
        mock_client.sessions.stop.side_effect = Exception("network error")
        s = HyperbrowserSession(_ws_endpoint="wss://x", _session_id="s-1", _client=mock_client)
        # Should log AND re-raise so callers know the stop failed
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

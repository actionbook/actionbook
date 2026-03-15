"""Tests for ActionbookProvider credential validation."""

from unittest.mock import MagicMock, patch

import pytest

from _plugin import ActionbookProvider


class TestActionbookProvider:
    """Test ActionbookProvider credential validation."""

    @patch("provider.actionbook.requests.get")
    def test_valid_credentials(self, mock_get):
        """Test credential validation passes when API responds OK."""
        mock_get.return_value = MagicMock(status_code=200)
        provider = ActionbookProvider()
        credentials = {"actionbook_api_key": "valid_key_123"}

        # Should not raise exception
        provider._validate_credentials(credentials)
        mock_get.assert_called_once()

    def test_missing_api_key_passes_validation(self):
        """Test that missing API key is accepted (public access)."""
        provider = ActionbookProvider()
        credentials = {}

        # Should not raise - validation still hits API but doesn't require key
        with patch("provider.actionbook.requests.get") as mock_get:
            mock_get.return_value = MagicMock(status_code=200)
            provider._validate_credentials(credentials)

    @patch("provider.actionbook.requests.get")
    def test_server_error_raises(self, mock_get):
        """Test that 5xx server error raises exception."""
        mock_get.return_value = MagicMock(status_code=500)
        provider = ActionbookProvider()

        with pytest.raises(Exception, match="server error"):
            provider._validate_credentials({})

    @patch("provider.actionbook.requests.get")
    def test_network_error_raises(self, mock_get):
        """Test that network errors raise with descriptive message."""
        import requests as _requests
        mock_get.side_effect = _requests.ConnectionError("DNS resolution failed")
        provider = ActionbookProvider()

        with pytest.raises(Exception, match="Cannot reach"):
            provider._validate_credentials({})

    @patch("provider.actionbook.requests.get")
    def test_invalid_actionbook_key_401_raises(self, mock_get):
        """Test that invalid Actionbook API key (401) raises exception."""
        mock_get.return_value = MagicMock(status_code=401)
        provider = ActionbookProvider()

        with pytest.raises(Exception, match="invalid"):
            provider._validate_credentials({"actionbook_api_key": "bad-key"})

    @patch("provider.actionbook.requests.get")
    def test_invalid_actionbook_key_403_raises(self, mock_get):
        """Test that invalid Actionbook API key (403) raises exception."""
        mock_get.return_value = MagicMock(status_code=403)
        provider = ActionbookProvider()

        with pytest.raises(Exception, match="invalid"):
            provider._validate_credentials({"actionbook_api_key": "bad-key"})

    @patch("provider.actionbook.requests.get")
    def test_actionbook_key_sent_as_x_api_key(self, mock_get):
        """Test that Actionbook API key is sent via X-API-Key header."""
        mock_get.return_value = MagicMock(status_code=200)
        provider = ActionbookProvider()

        provider._validate_credentials({"actionbook_api_key": "valid-key-123"})

        _, kwargs = mock_get.call_args
        assert kwargs["headers"]["X-API-Key"] == "valid-key-123"
        assert "Authorization" not in kwargs["headers"]

    def test_short_hyperbrowser_key_raises(self):
        """Test that too-short Hyperbrowser key raises exception."""
        provider = ActionbookProvider()

        with patch("provider.actionbook.requests.get") as mock_get:
            mock_get.return_value = MagicMock(status_code=200)
            with pytest.raises(Exception, match="too short"):
                provider._validate_credentials({"hyperbrowser_api_key": "abc"})

    def test_valid_hyperbrowser_key_passes(self):
        """Test that valid-length Hyperbrowser key passes."""
        provider = ActionbookProvider()

        with patch("provider.actionbook.requests.get") as mock_get:
            mock_get.return_value = MagicMock(status_code=200)
            provider._validate_credentials({"hyperbrowser_api_key": "hb-valid-key-12345"})

    @patch("provider.actionbook.requests.get")
    def test_html_response_raises_misroute_error(self, mock_get):
        """HTML responses should be treated as endpoint misconfiguration."""
        mock_get.return_value = MagicMock(
            status_code=200,
            text="<!DOCTYPE html><html><body>not api</body></html>",
            headers={"Content-Type": "text/html; charset=utf-8"},
        )
        provider = ActionbookProvider()

        with pytest.raises(Exception, match="HTML page"):
            provider._validate_credentials({})

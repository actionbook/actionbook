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

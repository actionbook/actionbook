"""Tests for ActionbookProvider credential validation."""

import sys
from pathlib import Path
from unittest.mock import Mock, patch

import pytest
import requests
from dify_plugin.errors.tool import ToolProviderCredentialValidationError

# Add parent directory to path for imports
sys.path.insert(0, str(Path(__file__).parent.parent))

from _plugin import ActionbookProvider


class TestActionbookProvider:
    """Test ActionbookProvider credential validation."""

    @patch("requests.get")
    def test_valid_credentials(self, mock_get):
        """Test credential validation with valid API key."""
        # Mock successful API response
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.text = "test result"
        mock_get.return_value = mock_response

        provider = ActionbookProvider()
        credentials = {"actionbook_api_key": "valid_key_123"}

        # Should not raise exception
        provider._validate_credentials(credentials)

        # Verify API call was made
        mock_get.assert_called_once()
        args, kwargs = mock_get.call_args
        assert "https://api.actionbook.dev/actions/search" in args[0]
        assert kwargs["headers"]["Authorization"] == "Bearer valid_key_123"

    def test_missing_api_key(self):
        """Test credential validation with missing API key."""
        provider = ActionbookProvider()
        credentials = {}

        with pytest.raises(ToolProviderCredentialValidationError) as exc_info:
            provider._validate_credentials(credentials)

        assert "API key is required" in str(exc_info.value)

    @patch("requests.get")
    def test_invalid_api_key(self, mock_get):
        """Test credential validation with invalid API key."""
        # Mock 401 Unauthorized response
        mock_response = Mock()
        mock_response.status_code = 401
        mock_get.return_value = mock_response

        provider = ActionbookProvider()
        credentials = {"actionbook_api_key": "invalid_key"}

        with pytest.raises(ToolProviderCredentialValidationError) as exc_info:
            provider._validate_credentials(credentials)

        assert "Invalid API key" in str(exc_info.value)
        assert "actionbook.dev/dashboard/api-keys" in str(exc_info.value)

    @patch("requests.get")
    def test_forbidden_api_key(self, mock_get):
        """Test credential validation with forbidden API key."""
        # Mock 403 Forbidden response
        mock_response = Mock()
        mock_response.status_code = 403
        mock_get.return_value = mock_response

        provider = ActionbookProvider()
        credentials = {"actionbook_api_key": "forbidden_key"}

        with pytest.raises(ToolProviderCredentialValidationError) as exc_info:
            provider._validate_credentials(credentials)

        assert "does not have permission" in str(exc_info.value)

    @patch("requests.get")
    def test_api_unavailable(self, mock_get):
        """Test credential validation when API is unavailable."""
        # Mock 500 Server Error response
        mock_response = Mock()
        mock_response.status_code = 500
        mock_get.return_value = mock_response

        provider = ActionbookProvider()
        credentials = {"actionbook_api_key": "valid_key"}

        with pytest.raises(ToolProviderCredentialValidationError) as exc_info:
            provider._validate_credentials(credentials)

        assert "currently unavailable" in str(exc_info.value)

    @patch("requests.get")
    def test_connection_error(self, mock_get):
        """Test credential validation with connection error."""
        # Mock connection error
        mock_get.side_effect = requests.ConnectionError("Network unreachable")

        provider = ActionbookProvider()
        credentials = {"actionbook_api_key": "valid_key"}

        with pytest.raises(ToolProviderCredentialValidationError) as exc_info:
            provider._validate_credentials(credentials)

        assert "Cannot connect to Actionbook API" in str(exc_info.value)

    @patch("requests.get")
    def test_timeout_error(self, mock_get):
        """Test credential validation with timeout."""
        # Mock timeout
        mock_get.side_effect = requests.Timeout()

        provider = ActionbookProvider()
        credentials = {"actionbook_api_key": "valid_key"}

        with pytest.raises(ToolProviderCredentialValidationError) as exc_info:
            provider._validate_credentials(credentials)

        assert "timed out" in str(exc_info.value)

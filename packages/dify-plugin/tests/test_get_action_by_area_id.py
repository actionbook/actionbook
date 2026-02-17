"""Tests for GetActionByAreaIdTool."""

import sys
from pathlib import Path
from unittest.mock import Mock, patch

import pytest
import requests

# Add parent directory to path for imports
sys.path.insert(0, str(Path(__file__).parent.parent))

from tools.get_action_by_area_id import GetActionByAreaIdTool


class TestGetActionByAreaIdTool:
    """Test GetActionByAreaIdTool functionality."""

    def setup_method(self):
        """Set up test fixtures."""
        self.tool = GetActionByAreaIdTool(api_key="test_key_123")

    @patch("requests.get")
    def test_get_action_success(self, mock_get):
        """Test successful action retrieval."""
        # Mock successful API response
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.text = """Site: github.com
Page: /login
Element: username-field
Selectors:
  - CSS: #login_field
  - XPath: //input[@name='login']
"""
        mock_get.return_value = mock_response

        tool_parameters = {"area_id": "github.com:login:username-field"}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "github.com" in result[0]
        assert "#login_field" in result[0]
        mock_get.assert_called_once()

    def test_missing_area_id_parameter(self):
        """Test error handling for missing area_id parameter."""
        tool_parameters = {}

        with pytest.raises(ValueError) as exc_info:
            list(self.tool._invoke(tool_parameters))

        assert "area_id" in str(exc_info.value).lower()
        assert "required" in str(exc_info.value).lower()

    def test_empty_area_id_parameter(self):
        """Test error handling for empty area_id parameter."""
        tool_parameters = {"area_id": "   "}

        with pytest.raises(ValueError) as exc_info:
            list(self.tool._invoke(tool_parameters))

        assert "required" in str(exc_info.value).lower()

    def test_invalid_area_id_format(self):
        """Test error handling for invalid area_id format."""
        tool_parameters = {"area_id": "invalid-format"}

        with pytest.raises(ValueError) as exc_info:
            list(self.tool._invoke(tool_parameters))

        assert "Invalid area_id format" in str(exc_info.value)
        assert "site:path:area" in str(exc_info.value)

    def test_area_id_with_only_two_parts(self):
        """Test error handling for area_id with insufficient parts."""
        tool_parameters = {"area_id": "github.com:login"}

        with pytest.raises(ValueError) as exc_info:
            list(self.tool._invoke(tool_parameters))

        assert "Invalid area_id format" in str(exc_info.value)

    @patch("requests.get")
    def test_action_not_found(self, mock_get):
        """Test handling of non-existent action."""
        # Mock 404 response
        mock_response = Mock()
        mock_response.status_code = 404
        mock_get.return_value = mock_response

        tool_parameters = {"area_id": "example.com:page:nonexistent"}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "Action not found" in result[0]
        assert "example.com:page:nonexistent" in result[0]

    @patch("requests.get")
    def test_invalid_api_key(self, mock_get):
        """Test handling of invalid API key."""
        # Mock 401 response
        mock_response = Mock()
        mock_response.status_code = 401
        mock_get.return_value = mock_response

        tool_parameters = {"area_id": "github.com:login:username"}

        with pytest.raises(ValueError) as exc_info:
            list(self.tool._invoke(tool_parameters))

        assert "Invalid API key" in str(exc_info.value)

    @patch("requests.get")
    def test_rate_limit_exceeded(self, mock_get):
        """Test handling of rate limit errors."""
        # Mock 429 response
        mock_response = Mock()
        mock_response.status_code = 429
        mock_get.return_value = mock_response

        tool_parameters = {"area_id": "github.com:login:username"}

        with pytest.raises(Exception) as exc_info:
            list(self.tool._invoke(tool_parameters))

        assert "Rate limit exceeded" in str(exc_info.value)

    @patch("requests.get")
    def test_api_unavailable(self, mock_get):
        """Test handling of API unavailability."""
        # Mock 500 response
        mock_response = Mock()
        mock_response.status_code = 500
        mock_get.return_value = mock_response

        tool_parameters = {"area_id": "github.com:login:username"}

        with pytest.raises(Exception) as exc_info:
            list(self.tool._invoke(tool_parameters))

        assert "currently unavailable" in str(exc_info.value)

    @patch("requests.get")
    def test_connection_error(self, mock_get):
        """Test handling of connection errors."""
        # Mock connection error
        mock_get.side_effect = requests.ConnectionError("Network unreachable")

        tool_parameters = {"area_id": "github.com:login:username"}

        with pytest.raises(ConnectionError) as exc_info:
            list(self.tool._invoke(tool_parameters))

        assert "Cannot connect" in str(exc_info.value)

    @patch("requests.get")
    def test_timeout_error(self, mock_get):
        """Test handling of timeout errors."""
        # Mock timeout
        mock_get.side_effect = requests.Timeout()

        tool_parameters = {"area_id": "github.com:login:username"}

        with pytest.raises(TimeoutError) as exc_info:
            list(self.tool._invoke(tool_parameters))

        assert "timed out" in str(exc_info.value)

    @patch("requests.get")
    def test_empty_response(self, mock_get):
        """Test handling of empty API response."""
        # Mock empty response
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.text = ""
        mock_get.return_value = mock_response

        tool_parameters = {"area_id": "github.com:login:username"}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "No details found" in result[0]

    def test_from_credentials_factory(self):
        """Test tool creation from credentials."""
        credentials = {"actionbook_api_key": "factory_key"}
        tool = GetActionByAreaIdTool.from_credentials(credentials)

        assert isinstance(tool, GetActionByAreaIdTool)
        assert tool.api_key == "factory_key"

    @patch("requests.get")
    def test_api_url_construction(self, mock_get):
        """Test that API URL is correctly constructed."""
        # Mock successful response
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.text = "test result"
        mock_get.return_value = mock_response

        tool_parameters = {"area_id": "github.com:login:username-field"}

        list(self.tool._invoke(tool_parameters))

        # Verify correct URL construction
        args, kwargs = mock_get.call_args
        assert "https://api.actionbook.dev/actions/github.com:login:username-field" in args[0]
        assert kwargs["headers"]["Authorization"] == "Bearer test_key_123"
        assert kwargs["headers"]["Accept"] == "text/plain"

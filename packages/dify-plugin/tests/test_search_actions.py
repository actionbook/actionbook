"""Tests for SearchActionsTool."""

import sys
from pathlib import Path
from unittest.mock import Mock, patch

import pytest
import requests

# Add parent directory to path for imports
sys.path.insert(0, str(Path(__file__).parent.parent))

from tools.search_actions import SearchActionsTool


class TestSearchActionsTool:
    """Test SearchActionsTool functionality."""

    def setup_method(self):
        """Set up test fixtures."""
        self.tool = SearchActionsTool(api_key="test_key_123")

    @patch("requests.get")
    def test_search_success(self, mock_get):
        """Test successful search query."""
        # Mock successful API response
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.text = "Area ID: github.com:login:username\nDescription: Login field"
        mock_get.return_value = mock_response

        tool_parameters = {"query": "GitHub login", "limit": 5}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "github.com:login:username" in result[0]
        mock_get.assert_called_once()

    @patch("requests.get")
    def test_search_with_domain_filter(self, mock_get):
        """Test search with domain filter."""
        # Mock successful API response
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.text = "Area ID: github.com:login:username"
        mock_get.return_value = mock_response

        tool_parameters = {"query": "login", "domain": "github.com", "limit": 3}

        list(self.tool._invoke(tool_parameters))

        # Verify domain parameter was passed
        args, kwargs = mock_get.call_args
        assert kwargs["params"]["domain"] == "github.com"
        assert kwargs["params"]["limit"] == 3

    def test_missing_query_parameter(self):
        """Test error handling for missing query parameter."""
        tool_parameters = {}

        with pytest.raises(ValueError) as exc_info:
            list(self.tool._invoke(tool_parameters))

        assert "query" in str(exc_info.value).lower()
        assert "required" in str(exc_info.value).lower()

    def test_empty_query_parameter(self):
        """Test error handling for empty query parameter."""
        tool_parameters = {"query": "   "}

        with pytest.raises(ValueError) as exc_info:
            list(self.tool._invoke(tool_parameters))

        assert "query" in str(exc_info.value).lower()

    def test_invalid_limit_parameter(self):
        """Test error handling for invalid limit parameter."""
        tool_parameters = {"query": "test", "limit": 0}

        with pytest.raises(ValueError) as exc_info:
            list(self.tool._invoke(tool_parameters))

        assert "limit" in str(exc_info.value).lower()

    def test_limit_too_large(self):
        """Test error handling for limit > 50."""
        tool_parameters = {"query": "test", "limit": 100}

        with pytest.raises(ValueError) as exc_info:
            list(self.tool._invoke(tool_parameters))

        assert "limit" in str(exc_info.value).lower()
        assert "50" in str(exc_info.value)

    @patch("requests.get")
    def test_no_results_found(self, mock_get):
        """Test handling of empty search results."""
        # Mock empty response
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.text = ""
        mock_get.return_value = mock_response

        tool_parameters = {"query": "nonexistent"}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "No results found" in result[0]

    @patch("requests.get")
    def test_invalid_api_key(self, mock_get):
        """Test handling of invalid API key."""
        # Mock 401 response
        mock_response = Mock()
        mock_response.status_code = 401
        mock_get.return_value = mock_response

        tool_parameters = {"query": "test"}

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

        tool_parameters = {"query": "test"}

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

        tool_parameters = {"query": "test"}

        with pytest.raises(Exception) as exc_info:
            list(self.tool._invoke(tool_parameters))

        assert "currently unavailable" in str(exc_info.value)

    @patch("requests.get")
    def test_connection_error(self, mock_get):
        """Test handling of connection errors."""
        # Mock connection error
        mock_get.side_effect = requests.ConnectionError("Network unreachable")

        tool_parameters = {"query": "test"}

        with pytest.raises(ConnectionError) as exc_info:
            list(self.tool._invoke(tool_parameters))

        assert "Cannot connect" in str(exc_info.value)

    @patch("requests.get")
    def test_timeout_error(self, mock_get):
        """Test handling of timeout errors."""
        # Mock timeout
        mock_get.side_effect = requests.Timeout()

        tool_parameters = {"query": "test"}

        with pytest.raises(TimeoutError) as exc_info:
            list(self.tool._invoke(tool_parameters))

        assert "timed out" in str(exc_info.value)

    def test_from_credentials_factory(self):
        """Test tool creation from credentials."""
        credentials = {"actionbook_api_key": "factory_key"}
        tool = SearchActionsTool.from_credentials(credentials)

        assert isinstance(tool, SearchActionsTool)
        assert tool.api_key == "factory_key"

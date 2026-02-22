"""Tests for SearchActionsTool."""

from unittest.mock import Mock, patch

import requests

from tools.search_actions import SearchActionsTool


def _make_tool(api_key: str = "test_key_123") -> SearchActionsTool:
    """Create a SearchActionsTool via the SDK's from_credentials classmethod."""
    return SearchActionsTool.from_credentials({"actionbook_api_key": api_key})


class TestSearchActionsTool:
    """Test SearchActionsTool functionality."""

    def setup_method(self):
        """Set up test fixtures."""
        self.tool = _make_tool()

    @patch("tools.search_actions.requests.get")
    def test_search_success(self, mock_get):
        """Test successful search query."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.text = "Area ID: github.com:login:username\nDescription: Login field"
        mock_get.return_value = mock_response

        tool_parameters = {"query": "GitHub login", "limit": 5}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "github.com:login:username" in result[0].message.text
        mock_get.assert_called_once()

    @patch("tools.search_actions.requests.get")
    def test_search_with_domain_filter(self, mock_get):
        """Test search with domain filter."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.text = "Area ID: github.com:login:username"
        mock_get.return_value = mock_response

        tool_parameters = {"query": "login", "domain": "github.com", "limit": 3}

        list(self.tool._invoke(tool_parameters))

        args, kwargs = mock_get.call_args
        assert kwargs["params"]["domain"] == "github.com"
        assert kwargs["params"]["page_size"] == 3

    def test_missing_query_parameter(self):
        """Test error message for missing query parameter."""
        tool_parameters = {}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "Error" in result[0].message.text
        assert "query" in result[0].message.text.lower()

    def test_empty_query_parameter(self):
        """Test error message for empty query parameter."""
        tool_parameters = {"query": "   "}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "Error" in result[0].message.text

    def test_none_query_parameter(self):
        """Test error message for None query parameter."""
        tool_parameters = {"query": None}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "Error" in result[0].message.text

    @patch("tools.search_actions.requests.get")
    def test_invalid_limit_defaults_to_10(self, mock_get):
        """Test that invalid limit parameter defaults to 10."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.text = "results"
        mock_get.return_value = mock_response

        tool_parameters = {"query": "test", "limit": 0}
        list(self.tool._invoke(tool_parameters))

        args, kwargs = mock_get.call_args
        assert kwargs["params"]["page_size"] == 10

    @patch("tools.search_actions.requests.get")
    def test_limit_too_large_defaults_to_10(self, mock_get):
        """Test that limit > 50 defaults to 10."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.text = "results"
        mock_get.return_value = mock_response

        tool_parameters = {"query": "test", "limit": 100}
        list(self.tool._invoke(tool_parameters))

        args, kwargs = mock_get.call_args
        assert kwargs["params"]["page_size"] == 10

    @patch("tools.search_actions.requests.get")
    def test_no_results_found(self, mock_get):
        """Test handling of empty search results."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.text = ""
        mock_get.return_value = mock_response

        tool_parameters = {"query": "nonexistent"}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        # Updated assertion to match new SSRF-aware error message
        assert "empty response" in result[0].message.text.lower()
        assert ("SSRF proxy" in result[0].message.text or
                "Self-hosted" in result[0].message.text)

    @patch("tools.search_actions.requests.get")
    def test_invalid_api_key(self, mock_get):
        """Test handling of invalid API key returns error message."""
        mock_response = Mock()
        mock_response.status_code = 401
        mock_get.return_value = mock_response

        tool_parameters = {"query": "test"}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "Unauthorized" in result[0].message.text

    @patch("tools.search_actions.requests.get")
    def test_rate_limit_exceeded(self, mock_get):
        """Test handling of rate limit errors."""
        mock_response = Mock()
        mock_response.status_code = 429
        mock_get.return_value = mock_response

        tool_parameters = {"query": "test"}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "Rate limit" in result[0].message.text

    @patch("tools.search_actions.requests.get")
    def test_api_unavailable(self, mock_get):
        """Test handling of API unavailability."""
        mock_response = Mock()
        mock_response.status_code = 500
        mock_get.return_value = mock_response

        tool_parameters = {"query": "test"}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "server error" in result[0].message.text

    @patch("tools.search_actions.requests.get")
    def test_connection_error(self, mock_get):
        """Test handling of connection errors yields message."""
        mock_get.side_effect = requests.ConnectionError("Network unreachable")

        tool_parameters = {"query": "test"}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "Cannot connect" in result[0].message.text

    @patch("tools.search_actions.requests.get")
    def test_timeout_error(self, mock_get):
        """Test handling of timeout errors yields message."""
        mock_get.side_effect = requests.Timeout()

        tool_parameters = {"query": "test"}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "timed out" in result[0].message.text

    @patch("tools.search_actions.requests.get")
    def test_unexpected_error(self, mock_get):
        """Test handling of unexpected errors yields message."""
        mock_get.side_effect = RuntimeError("something broke")

        tool_parameters = {"query": "test"}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "unexpected error" in result[0].message.text.lower()

    @patch("tools.search_actions.requests.get")
    def test_base_exception_gevent_timeout(self, mock_get):
        """Test handling of BaseException (e.g., gevent.Timeout) yields message.

        Critical test: Ensures that even BaseException (which bypasses normal Exception)
        still yields an error message to the user instead of causing empty response.
        """
        # Simulate gevent.Timeout which inherits from BaseException
        class MockGeventTimeout(BaseException):
            """Mock gevent.Timeout for testing."""
            pass

        mock_get.side_effect = MockGeventTimeout("greenlet timeout")

        tool_parameters = {"query": "test"}

        result = list(self.tool._invoke(tool_parameters))

        # Critical assertion: message must be yielded even for BaseException
        assert len(result) == 1, "BaseException must still yield error message"
        assert "system-level error" in result[0].message.text.lower()
        assert "MockGeventTimeout" in result[0].message.text

    @patch("tools.search_actions.requests.get")
    def test_search_without_api_key(self, mock_get):
        """Test search works without API key (public access)."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.text = "Area ID: github.com:login:username"
        mock_get.return_value = mock_response

        tool = _make_tool(api_key="")
        tool_parameters = {"query": "GitHub login"}

        result = list(tool._invoke(tool_parameters))

        assert len(result) == 1
        args, kwargs = mock_get.call_args
        assert "X-API-Key" not in kwargs["headers"]
        assert kwargs["headers"]["Accept"] == "text/plain"

    def test_from_credentials_factory(self):
        """Test tool creation from credentials."""
        credentials = {"actionbook_api_key": "factory_key"}
        tool = SearchActionsTool.from_credentials(credentials)

        assert isinstance(tool, SearchActionsTool)
        assert tool.runtime.credentials["actionbook_api_key"] == "factory_key"

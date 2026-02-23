"""Tests for SearchActionsTool."""

from unittest.mock import Mock, patch

import requests

from tools.search_actions import SearchActionsTool


class _FakeSystemError(BaseException):
    """Simulate gevent.Timeout / non-Exception runtime errors."""


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
    def test_baseexception_error(self, mock_get):
        """Test handling of non-Exception errors yields system-level message."""
        mock_get.side_effect = _FakeSystemError("gevent timeout")

        tool_parameters = {"query": "test"}
        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "system-level error" in result[0].message.text.lower()

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

    @patch("tools.search_actions.requests.get")
    def test_http_403_status(self, mock_get):
        """Test handling of HTTP 403 Forbidden status."""
        mock_response = Mock()
        mock_response.status_code = 403
        mock_get.return_value = mock_response

        tool_parameters = {"query": "test"}
        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "403" in result[0].message.text

    @patch("tools.search_actions.requests.get")
    def test_connection_error_ssl(self, mock_get):
        """Test SSL-specific connection error branch."""
        mock_get.side_effect = requests.ConnectionError("SSL certificate verify failed")

        tool_parameters = {"query": "test"}
        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "SSL" in result[0].message.text

    @patch("tools.search_actions.requests.get")
    def test_connection_error_refused(self, mock_get):
        """Test connection-refused branch."""
        mock_get.side_effect = requests.ConnectionError("Connection refused")

        tool_parameters = {"query": "test"}
        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "refused" in result[0].message.text.lower()

    @patch("tools.search_actions.requests.get")
    def test_connection_error_timeout(self, mock_get):
        """Test connection timeout branch."""
        mock_get.side_effect = requests.ConnectionError("Connection timeout")

        tool_parameters = {"query": "test"}
        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "timeout" in result[0].message.text.lower()

    def test_float_limit_cast_to_int(self):
        """Test that float limit is cast to int before validation."""
        tool = _make_tool()
        with patch("tools.search_actions.requests.get") as mock_get:
            mock_response = Mock()
            mock_response.status_code = 200
            mock_response.text = "results"
            mock_get.return_value = mock_response

            # Float 5.7 should be cast to int 5
            list(tool._invoke({"query": "test", "limit": 5.7}))

            _, kwargs = mock_get.call_args
            assert kwargs["params"]["page_size"] == 5

    def test_string_limit_defaults_to_10(self):
        """Test that non-numeric string limit defaults to 10."""
        tool = _make_tool()
        with patch("tools.search_actions.requests.get") as mock_get:
            mock_response = Mock()
            mock_response.status_code = 200
            mock_response.text = "results"
            mock_get.return_value = mock_response

            list(tool._invoke({"query": "test", "limit": "abc"}))

            _, kwargs = mock_get.call_args
            assert kwargs["params"]["page_size"] == 10

    def test_from_credentials_factory(self):
        """Test tool creation from credentials."""
        credentials = {"actionbook_api_key": "factory_key"}
        tool = SearchActionsTool.from_credentials(credentials)

        assert isinstance(tool, SearchActionsTool)
        assert tool.runtime.credentials["actionbook_api_key"] == "factory_key"

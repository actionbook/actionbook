"""Tests for GetActionByAreaIdTool."""

from unittest.mock import Mock, patch

import pytest
import requests

from tools.get_action_by_area_id import GetActionByAreaIdTool


class _FakeSystemError(BaseException):
    """Simulate gevent.Timeout / non-Exception runtime errors."""


def _make_tool(api_key: str = "test_key_123") -> GetActionByAreaIdTool:
    """Create a GetActionByAreaIdTool via the SDK's from_credentials classmethod."""
    return GetActionByAreaIdTool.from_credentials({"actionbook_api_key": api_key})


class TestGetActionByAreaIdTool:
    """Test GetActionByAreaIdTool functionality."""

    def setup_method(self):
        """Set up test fixtures."""
        self.tool = _make_tool()

    @patch("tools.get_action_by_area_id.requests.get")
    def test_get_action_success(self, mock_get):
        """Test successful action retrieval."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.text = """Site: github.com
Page: /login
Element: username-field
Selectors:
  - CSS: #login_field
  - XPath: //input[@name='login']
"""
        mock_response.headers = {"Content-Type": "text/plain"}
        mock_get.return_value = mock_response

        tool_parameters = {"area_id": "github-site:login:username-field"}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "#login_field" in result[0].message.text
        assert "username-field" in result[0].message.text
        mock_get.assert_called_once()

    def test_missing_area_id_parameter(self):
        """Test error message for missing area_id parameter."""
        tool_parameters = {}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "Error" in result[0].message.text
        assert "area_id" in result[0].message.text.lower()

    def test_empty_area_id_parameter(self):
        """Test error message for empty area_id parameter."""
        tool_parameters = {"area_id": "   "}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "Error" in result[0].message.text

    def test_invalid_area_id_format(self):
        """Test error message for invalid area_id format."""
        tool_parameters = {"area_id": "invalid-format"}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "Invalid area_id format" in result[0].message.text
        assert "site:path:area" in result[0].message.text

    def test_area_id_with_only_two_parts(self):
        """Test error message for area_id with insufficient parts."""
        tool_parameters = {"area_id": "github.com:login"}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "Invalid area_id format" in result[0].message.text

    @patch("tools.get_action_by_area_id.requests.get")
    def test_action_not_found(self, mock_get):
        """Test handling of non-existent action."""
        mock_response = Mock()
        mock_response.status_code = 404
        mock_response.text = ""
        mock_response.headers = {"Content-Type": "application/json"}
        mock_get.return_value = mock_response

        tool_parameters = {"area_id": "example.com:page:nonexistent"}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "Action not found" in result[0].message.text
        assert "example.com:page:nonexistent" in result[0].message.text

    @patch("tools.get_action_by_area_id.requests.get")
    def test_invalid_api_key(self, mock_get):
        """Test handling of invalid API key returns error message."""
        mock_response = Mock()
        mock_response.status_code = 401
        mock_response.text = ""
        mock_response.headers = {"Content-Type": "application/json"}
        mock_get.return_value = mock_response

        tool_parameters = {"area_id": "github.com:login:username"}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "invalid" in result[0].message.text

    @patch("tools.get_action_by_area_id.requests.get")
    def test_rate_limit_exceeded(self, mock_get):
        """Test handling of rate limit errors."""
        mock_response = Mock()
        mock_response.status_code = 429
        mock_response.text = ""
        mock_response.headers = {"Content-Type": "application/json"}
        mock_get.return_value = mock_response

        tool_parameters = {"area_id": "github.com:login:username"}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "Rate limit" in result[0].message.text

    @patch("tools.get_action_by_area_id.requests.get")
    def test_api_unavailable(self, mock_get):
        """Test handling of API unavailability."""
        mock_response = Mock()
        mock_response.status_code = 500
        mock_response.text = ""
        mock_response.headers = {"Content-Type": "application/json"}
        mock_get.return_value = mock_response

        tool_parameters = {"area_id": "github.com:login:username"}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "server error" in result[0].message.text.lower()

    @patch("tools.get_action_by_area_id.requests.get")
    def test_connection_error(self, mock_get):
        """Test handling of connection errors yields message."""
        mock_get.side_effect = requests.ConnectionError("Network unreachable")

        tool_parameters = {"area_id": "github.com:login:username"}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "ConnectionError" in result[0].message.text

    @patch("tools.get_action_by_area_id.requests.get")
    def test_timeout_error(self, mock_get):
        """Test handling of timeout errors yields message."""
        mock_get.side_effect = requests.Timeout()

        tool_parameters = {"area_id": "github.com:login:username"}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "timed out" in result[0].message.text.lower()

    @patch("tools.get_action_by_area_id.requests.get")
    def test_unexpected_error(self, mock_get):
        """Test handling of unexpected errors yields message."""
        mock_get.side_effect = RuntimeError("something broke")

        tool_parameters = {"area_id": "github.com:login:username"}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "RuntimeError" in result[0].message.text
        assert "something broke" in result[0].message.text

    @patch("tools.get_action_by_area_id.requests.get")
    def test_baseexception_propagates(self, mock_get):
        """Test that non-Exception BaseException subclasses propagate uncaught."""
        mock_get.side_effect = _FakeSystemError("gevent timeout")

        tool_parameters = {"area_id": "github.com:login:username"}
        with pytest.raises(_FakeSystemError, match="gevent timeout"):
            list(self.tool._invoke(tool_parameters))

    @patch("tools.get_action_by_area_id.requests.get")
    def test_empty_response(self, mock_get):
        """Test handling of empty API response."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.text = ""
        mock_response.headers = {"Content-Type": "text/plain"}
        mock_get.return_value = mock_response

        tool_parameters = {"area_id": "github.com:login:username"}

        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "empty response" in result[0].message.text.lower()
        assert ("SSRF proxy" in result[0].message.text or
                "Self-hosted" in result[0].message.text)

    @patch("tools.get_action_by_area_id.requests.get")
    def test_get_action_without_api_key(self, mock_get):
        """Test action retrieval works without API key (public access)."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.text = "Site: github.com\nElement: username-field"
        mock_response.headers = {"Content-Type": "text/plain"}
        mock_get.return_value = mock_response

        tool = _make_tool(api_key="")
        tool_parameters = {"area_id": "github.com:login:username-field"}

        result = list(tool._invoke(tool_parameters))

        assert len(result) == 1
        args, kwargs = mock_get.call_args
        assert "X-API-Key" not in kwargs["headers"]
        assert "Authorization" not in kwargs["headers"]
        assert kwargs["headers"]["Accept"] == "text/plain"

    @patch("tools.get_action_by_area_id.requests.get")
    def test_api_url_construction(self, mock_get):
        """Test that API URL is correctly constructed."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.text = "test result"
        mock_response.headers = {"Content-Type": "text/plain"}
        mock_get.return_value = mock_response

        tool_parameters = {"area_id": "github.com:login:username-field"}

        list(self.tool._invoke(tool_parameters))

        args, kwargs = mock_get.call_args
        assert "https://api.actionbook.dev/api/get_action_by_area_id" in args[0]
        assert kwargs["params"]["area_id"] == "github.com:login:username-field"
        assert kwargs["headers"]["Accept"] == "text/plain"

    @patch("tools.get_action_by_area_id.requests.get")
    def test_http_403_status(self, mock_get):
        """Test handling of HTTP 403 — treated as invalid API key."""
        mock_response = Mock()
        mock_response.status_code = 403
        mock_response.text = ""
        mock_response.headers = {"Content-Type": "application/json"}
        mock_get.return_value = mock_response

        tool_parameters = {"area_id": "github.com:login:username"}
        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "invalid" in result[0].message.text
        assert "403" in result[0].message.text

    @patch("tools.get_action_by_area_id.requests.get")
    def test_connection_error_ssl(self, mock_get):
        """Test SSL-specific connection error includes error message."""
        mock_get.side_effect = requests.ConnectionError("SSL certificate verify failed")

        tool_parameters = {"area_id": "github.com:login:username"}
        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "SSL" in result[0].message.text

    @patch("tools.get_action_by_area_id.requests.get")
    def test_connection_error_refused(self, mock_get):
        """Test connection-refused error includes error message."""
        mock_get.side_effect = requests.ConnectionError("Connection refused")

        tool_parameters = {"area_id": "github.com:login:username"}
        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "refused" in result[0].message.text.lower()

    @patch("tools.get_action_by_area_id.requests.get")
    def test_connection_error_timeout(self, mock_get):
        """Test connection timeout error includes error message."""
        mock_get.side_effect = requests.ConnectionError("Connection timeout")

        tool_parameters = {"area_id": "github.com:login:username"}
        result = list(self.tool._invoke(tool_parameters))

        assert len(result) == 1
        assert "timeout" in result[0].message.text.lower()

    def test_from_credentials_factory(self):
        """Test tool creation from credentials."""
        credentials = {"actionbook_api_key": "factory_key"}
        tool = GetActionByAreaIdTool.from_credentials(credentials)

        assert isinstance(tool, GetActionByAreaIdTool)
        assert tool.runtime.credentials["actionbook_api_key"] == "factory_key"

    @patch("tools.get_action_by_area_id.requests.get")
    def test_html_response_returns_misroute_error(self, mock_get):
        """HTML response should return actionable base URL guidance."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.text = "<!DOCTYPE html><html><body>Redirecting</body></html>"
        mock_response.headers = {"Content-Type": "text/html; charset=utf-8"}
        mock_get.return_value = mock_response

        result = list(self.tool._invoke({"area_id": "github.com:login:username"}))

        assert len(result) == 1
        assert "HTML page" in result[0].message.text
        assert "ACTIONBOOK_API_URL" in result[0].message.text

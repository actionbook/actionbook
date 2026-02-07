"""Search Actions Tool - Query verified website selectors."""

from collections.abc import Generator
from typing import Any

import requests
from dify_plugin import Tool


class SearchActionsTool(Tool):
    """Search for website actions by keyword or context."""

    API_BASE_URL = "https://api.actionbook.dev"

    @classmethod
    def from_credentials(cls, credentials: dict[str, Any]) -> "SearchActionsTool":
        """Create tool instance from provider credentials."""
        return cls(api_key=credentials.get("actionbook_api_key"))

    def __init__(self, api_key: str):
        """
        Initialize SearchActionsTool.

        Args:
            api_key: Actionbook API key for authentication
        """
        self.api_key = api_key

    def _invoke(self, tool_parameters: dict[str, Any]) -> Generator[str, None, None]:
        """
        Execute search query against Actionbook API.

        Args:
            tool_parameters: Dict with keys:
                - query (required): Search keyword or context
                - domain (optional): Filter by website domain
                - limit (optional): Max results (default: 10, max: 50)

        Yields:
            Search results as formatted text

        Raises:
            ValueError: If query parameter is missing or invalid
            ConnectionError: If API request fails
            TimeoutError: If request times out
        """
        query = tool_parameters.get("query", "").strip()
        domain = tool_parameters.get("domain")
        limit = tool_parameters.get("limit", 10)

        # Validation
        if not query:
            raise ValueError("'query' parameter is required and cannot be empty")

        if limit < 1 or limit > 50:
            raise ValueError("'limit' must be between 1 and 50")

        # Build request parameters
        params = {"query": query, "limit": limit}
        if domain:
            params["domain"] = domain

        # Call Actionbook API
        try:
            response = requests.get(
                f"{self.API_BASE_URL}/actions/search",
                headers={
                    "Authorization": f"Bearer {self.api_key}",
                    "Accept": "text/plain",  # Request text format for LLM
                },
                params=params,
                timeout=30,
            )

            # Error handling
            if response.status_code == 401:
                raise ValueError("Invalid API key")
            elif response.status_code == 429:
                raise Exception("Rate limit exceeded. Please try again later.")
            elif response.status_code >= 500:
                raise Exception("Actionbook API is currently unavailable")
            elif response.status_code != 200:
                raise Exception(f"API request failed with status {response.status_code}")

            # Stream response (Dify expects generators)
            result_text = response.text

            if not result_text or result_text.strip() == "":
                yield "No results found for your query. Try different keywords or remove domain filter."
            else:
                yield result_text

        except requests.ConnectionError as e:
            raise ConnectionError(f"Cannot connect to Actionbook API: {str(e)}") from e
        except requests.Timeout as e:
            raise TimeoutError("Request to Actionbook API timed out after 30 seconds") from e

"""Get Action By Area ID Tool - Retrieve full action details."""

from collections.abc import Generator
from typing import Any

import requests
from dify_plugin import Tool


class GetActionByAreaIdTool(Tool):
    """Retrieve complete action details by area ID."""

    API_BASE_URL = "https://api.actionbook.dev"

    @classmethod
    def from_credentials(cls, credentials: dict[str, Any]) -> "GetActionByAreaIdTool":
        """Create tool instance from provider credentials."""
        return cls(api_key=credentials.get("actionbook_api_key"))

    def __init__(self, api_key: str):
        """
        Initialize GetActionByAreaIdTool.

        Args:
            api_key: Actionbook API key for authentication
        """
        self.api_key = api_key

    def _invoke(self, tool_parameters: dict[str, Any]) -> Generator[str, None, None]:
        """
        Fetch action details by area ID.

        Args:
            tool_parameters: Dict with keys:
                - area_id (required): Area ID in format "site:path:area"

        Yields:
            Full action details as formatted text

        Raises:
            ValueError: If area_id is missing or malformed
            ConnectionError: If API request fails
            TimeoutError: If request times out
        """
        area_id = tool_parameters.get("area_id", "").strip()

        # Validation
        if not area_id:
            raise ValueError("'area_id' parameter is required")

        # Basic format validation (site:path:area)
        parts = area_id.split(":")
        if len(parts) < 3:
            raise ValueError(
                "Invalid area_id format. Expected 'site:path:area' "
                f"(e.g., 'github.com:login:email-input'), got: {area_id}"
            )

        # Call Actionbook API
        try:
            response = requests.get(
                f"{self.API_BASE_URL}/actions/{area_id}",
                headers={
                    "Authorization": f"Bearer {self.api_key}",
                    "Accept": "text/plain",
                },
                timeout=30,
            )

            # Error handling
            if response.status_code == 404:
                yield f"Action not found for area_id: {area_id}"
                return
            elif response.status_code == 401:
                raise ValueError("Invalid API key")
            elif response.status_code == 429:
                raise Exception("Rate limit exceeded. Please try again later.")
            elif response.status_code >= 500:
                raise Exception("Actionbook API is currently unavailable")
            elif response.status_code != 200:
                raise Exception(f"API request failed with status {response.status_code}")

            # Stream response
            result_text = response.text

            if not result_text or result_text.strip() == "":
                yield f"No details found for area_id: {area_id}"
            else:
                yield result_text

        except requests.ConnectionError as e:
            raise ConnectionError(f"Cannot connect to Actionbook API: {str(e)}") from e
        except requests.Timeout as e:
            raise TimeoutError("Request to Actionbook API timed out after 30 seconds") from e

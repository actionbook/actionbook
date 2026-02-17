"""Actionbook Dify Plugin - Tool Provider Implementation."""

from typing import Any

import requests
from dify_plugin import ToolProvider
from dify_plugin.errors.tool import ToolProviderCredentialValidationError


class ActionbookProvider(ToolProvider):
    """Manages credentials and tool instantiation for Actionbook."""

    def _validate_credentials(self, credentials: dict[str, Any]) -> None:
        """
        Validate API key by making a test request.

        Args:
            credentials: Dict with 'actionbook_api_key' field

        Raises:
            ToolProviderCredentialValidationError: If API key is invalid
        """
        api_key = credentials.get("actionbook_api_key")

        if not api_key:
            raise ToolProviderCredentialValidationError("API key is required")

        # Test API key with a minimal search request
        try:
            response = requests.get(
                "https://api.actionbook.dev/actions/search",
                headers={
                    "Authorization": f"Bearer {api_key}",
                    "Accept": "text/plain",
                },
                params={"query": "test", "limit": 1},
                timeout=10,
            )

            if response.status_code == 401:
                raise ToolProviderCredentialValidationError(
                    "Invalid API key. Get your key at https://actionbook.dev/dashboard/api-keys"
                )
            elif response.status_code == 403:
                raise ToolProviderCredentialValidationError(
                    "API key does not have permission to access this resource"
                )
            elif response.status_code >= 500:
                raise ToolProviderCredentialValidationError(
                    "Actionbook API is currently unavailable. Please try again later."
                )
            elif response.status_code != 200:
                raise ToolProviderCredentialValidationError(
                    f"API key validation failed with status {response.status_code}"
                )

        except requests.ConnectionError as e:
            raise ToolProviderCredentialValidationError(
                f"Cannot connect to Actionbook API: {str(e)}"
            ) from e
        except requests.Timeout as e:
            raise ToolProviderCredentialValidationError(
                "Actionbook API request timed out. Please try again."
            ) from e

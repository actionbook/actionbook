"""Actionbook Dify Plugin - Tool Provider Implementation."""

import logging
from typing import Any

import requests
from dify_plugin import ToolProvider

from constants import API_BASE_URL

logger = logging.getLogger(__name__)


class ActionbookProvider(ToolProvider):
    """Manages tool instantiation for Actionbook."""

    def _validate_credentials(self, credentials: dict[str, Any]) -> None:
        """Validate provider by performing a lightweight API health check."""
        try:
            response = requests.get(
                f"{API_BASE_URL}/api/search_actions",
                params={"query": "test", "page_size": 1},
                headers={"Accept": "text/plain"},
                timeout=10,
            )
            if response.status_code >= 500:
                raise Exception(
                    f"Actionbook API returned server error ({response.status_code})"
                )
        except requests.ConnectionError as e:
            raise Exception(
                f"Cannot reach Actionbook API at {API_BASE_URL}: {e}"
            ) from e
        except requests.Timeout:
            raise Exception(
                "Actionbook API health check timed out."
            ) from None

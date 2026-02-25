"""Actionbook Dify Plugin - Tool Provider Implementation."""

import logging
from typing import Any

import requests
from dify_plugin import ToolProvider

from constants import API_BASE_URL
from utils.http_guard import build_html_misroute_message, looks_like_html_response

logger = logging.getLogger(__name__)


class ActionbookProvider(ToolProvider):
    """Manages tool instantiation for Actionbook."""

    def _validate_credentials(self, credentials: dict[str, Any]) -> None:
        """Validate provider by performing a lightweight API health check."""
        request_url = f"{API_BASE_URL}/api/search_actions"
        try:
            response = requests.get(
                request_url,
                params={"query": "test", "page_size": 1},
                headers={"Accept": "text/plain"},
                timeout=10,
            )
            body = response.text or ""
            if looks_like_html_response(response, body):
                raise Exception(build_html_misroute_message(API_BASE_URL, request_url))
            if response.status_code >= 500:
                raise Exception(
                    f"Actionbook API returned server error ({response.status_code})"
                )
            if response.status_code != 200:
                raise Exception(
                    f"Actionbook API validation failed with status {response.status_code}. "
                    f"Check ACTIONBOOK_API_URL (current: {API_BASE_URL})."
                )
        except requests.ConnectionError as e:
            raise Exception(
                f"Cannot reach Actionbook API at {API_BASE_URL}: {e}"
            ) from e
        except requests.Timeout:
            raise Exception(
                "Actionbook API health check timed out."
            ) from None

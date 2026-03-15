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
        """Validate provider credentials.

        - Always performs an API health check.
        - If actionbook_api_key is provided, validates it with an authenticated request.
        - If hyperbrowser_api_key is provided, does a basic format check.
        """
        actionbook_key = (credentials.get("actionbook_api_key") or "").strip()
        hyperbrowser_key = (credentials.get("hyperbrowser_api_key") or "").strip()

        # --- Validate Actionbook API connectivity (and key if provided) ---
        request_url = f"{API_BASE_URL}/api/search_actions"
        headers: dict[str, str] = {"Accept": "text/plain"}
        if actionbook_key:
            headers["X-API-Key"] = actionbook_key

        try:
            response = requests.get(
                request_url,
                params={"query": "test", "page_size": 1},
                headers=headers,
                timeout=10,
            )
            body = response.text or ""
            if looks_like_html_response(response, body):
                raise Exception(build_html_misroute_message(API_BASE_URL, request_url))
            if response.status_code in (401, 403):
                raise Exception(
                    f"Actionbook API Key is invalid ({response.status_code}). "
                    "Please check your key or leave it empty to use the free tier."
                )
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

        # --- Basic format check for Hyperbrowser key (if provided) ---
        if hyperbrowser_key and len(hyperbrowser_key) < 8:
            raise Exception(
                "Hyperbrowser API Key appears too short. "
                "Please check your key from https://app.hyperbrowser.ai/"
            )

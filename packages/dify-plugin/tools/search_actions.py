"""Search Actions Tool - Query verified website selectors."""

import logging
from collections.abc import Generator
from typing import Any

import requests
from dify_plugin import Tool
from dify_plugin.entities.tool import ToolInvokeMessage

from constants import API_BASE_URL
from utils.http_guard import build_html_misroute_message, looks_like_html_response

logger = logging.getLogger(__name__)


class SearchActionsTool(Tool):
    """Search for website actions by keyword or context."""

    def _invoke(self, tool_parameters: dict[str, Any]) -> Generator[ToolInvokeMessage, None, None]:
        """
        Execute search query against Actionbook API.

        Args:
            tool_parameters: Dict with keys:
                - query (required): Search keyword or context
                - domain (optional): Filter by website domain
                - limit (optional): Max results (default: 10, max: 50)

        Yields:
            ToolInvokeMessage with search results as formatted text
        """
        try:
            query = tool_parameters.get("query", "").strip() if tool_parameters.get("query") else ""
            domain = tool_parameters.get("domain")
            limit = tool_parameters.get("limit", 10)

            if not query:
                yield self.create_text_message("Error: 'query' parameter is required and cannot be empty.")
                return

            try:
                limit = int(limit)
            except (TypeError, ValueError):
                limit = 10
            if limit < 1 or limit > 50:
                limit = 10

            params: dict[str, Any] = {"query": query, "page_size": int(limit)}
            if domain:
                params["domain"] = domain

            headers: dict[str, str] = {"Accept": "text/plain"}
            actionbook_key = (self.runtime.credentials.get("actionbook_api_key") or "").strip()
            if actionbook_key:
                headers["X-API-Key"] = actionbook_key

            request_url = f"{API_BASE_URL}/api/search_actions"

            response = requests.get(
                request_url,
                headers=headers,
                params=params,
                timeout=30,
            )
            result_text = response.text or ""

            if looks_like_html_response(response, result_text):
                yield self.create_text_message(
                    build_html_misroute_message(API_BASE_URL, request_url)
                )
                return

            if response.status_code in (401, 403):
                yield self.create_text_message(
                    f"Error: API key is invalid ({response.status_code}). "
                    "Please check your Actionbook API Key or leave it empty to use the free tier."
                )
                return
            elif response.status_code == 429:
                yield self.create_text_message(
                    "Error: Rate limit exceeded (429). Please retry after a short delay."
                )
                return
            elif response.status_code >= 500:
                yield self.create_text_message(
                    f"Error: Actionbook API returned server error ({response.status_code})."
                )
                return
            elif response.status_code != 200:
                yield self.create_text_message(
                    f"Error: API request failed with status {response.status_code}."
                )
                return

            if not result_text or result_text.strip() == "":
                yield self.create_text_message(
                    "Error: Received empty response from Actionbook API.\n\n"
                    "This likely indicates Dify Cloud's SSRF proxy is intercepting "
                    "and returning an empty response for requests to actionbook.dev.\n\n"
                    "Solutions:\n"
                    "1. Use Dify Self-hosted (recommended for full control)\n"
                    "2. Contact Dify support to whitelist api.actionbook.dev\n"
                    "3. Check if HTTP_PROXY/HTTPS_PROXY is set in the environment"
                )
            else:
                yield self.create_text_message(result_text)

        except requests.ConnectionError as e:
            logger.exception("Connection error calling Actionbook API")
            yield self.create_text_message(
                f"Error: ConnectionError to {API_BASE_URL}.\n"
                f"{type(e).__name__}: {e}\n\n"
                "Dify Cloud likely restricts external API calls via SSRF proxy. "
                "actionbook.dev may not be whitelisted."
            )
        except requests.Timeout:
            logger.exception("Timeout calling Actionbook API")
            yield self.create_text_message(
                f"Error: Request to {API_BASE_URL} timed out after 30 seconds.\n\n"
                "This may indicate network restrictions in Dify Cloud."
            )
        except Exception:
            logger.exception("Unexpected error in search_actions")
            yield self.create_text_message(
                "Error: An unexpected error occurred while searching actions."
            )

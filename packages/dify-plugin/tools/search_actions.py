"""Search Actions Tool - Query verified website selectors."""

import logging
import sys
from collections.abc import Generator
from typing import Any

import requests
from dify_plugin import Tool
from dify_plugin.entities.tool import ToolInvokeMessage

from constants import API_BASE_URL

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
        # Debug: log that _invoke was called (visible in Dify Cloud plugin logs)
        print(f"[search_actions] _invoke called with parameters: {tool_parameters}", file=sys.stderr, flush=True)

        try:
            query = tool_parameters.get("query", "").strip() if tool_parameters.get("query") else ""
            domain = tool_parameters.get("domain")
            limit = tool_parameters.get("limit", 10)

            if not query:
                yield self.create_text_message("Error: 'query' parameter is required and cannot be empty.")
                return

            if not isinstance(limit, (int, float)) or limit < 1 or limit > 50:
                limit = 10

            params: dict[str, Any] = {"query": query, "page_size": int(limit)}
            if domain:
                params["domain"] = domain

            headers = {"Accept": "text/plain"}

            print(f"[search_actions] Making request to {API_BASE_URL}/api/search_actions with params={params}", file=sys.stderr, flush=True)

            response = requests.get(
                f"{API_BASE_URL}/api/search_actions",
                headers=headers,
                params=params,
                timeout=30,
            )

            print(f"[search_actions] Response status={response.status_code}", file=sys.stderr, flush=True)

            if response.status_code == 401:
                yield self.create_text_message("Error: Unauthorized (401). API key may be invalid.")
                return
            elif response.status_code == 429:
                yield self.create_text_message("Error: Rate limit exceeded (429). Please try again later.")
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

            result_text = response.text

            if not result_text or result_text.strip() == "":
                yield self.create_text_message(
                    "Error: Received empty response from Actionbook API. "
                    "This often indicates that Dify Cloud's SSRF proxy is blocking the request. "
                    "actionbook.dev may not be in the whitelist. "
                    "\n\nSolutions:\n"
                    "1. Use Dify Self-hosted (recommended for full control)\n"
                    "2. Contact Dify support to whitelist actionbook.dev\n"
                    "3. Check plugin logs in Dify for more details"
                )
            else:
                yield self.create_text_message(result_text)

        except requests.ConnectionError as e:
            logger.exception("Connection error calling Actionbook API")
            error_msg = str(e).lower()

            # Diagnose specific connection issues
            if "certificate" in error_msg or "ssl" in error_msg:
                yield self.create_text_message(
                    f"Error: SSL/Certificate error connecting to {API_BASE_URL}. "
                    "The API endpoint may be blocked by Dify Cloud's SSRF proxy. "
                    "Consider using Dify Self-hosted or contact Dify support to whitelist actionbook.dev."
                )
            elif "refused" in error_msg or "forbidden" in error_msg:
                yield self.create_text_message(
                    f"Error: Connection refused to {API_BASE_URL}. "
                    "Dify Cloud's SSRF proxy is blocking external API access. "
                    "Solutions: (1) Use Dify Self-hosted, or (2) Contact Dify to whitelist actionbook.dev."
                )
            elif "timeout" in error_msg:
                yield self.create_text_message(
                    f"Error: Connection timeout to {API_BASE_URL}. "
                    "Network may be restricted in Dify Cloud environment. "
                    "Try Dify Self-hosted for unrestricted network access."
                )
            else:
                yield self.create_text_message(
                    f"Error: Cannot connect to {API_BASE_URL}. "
                    "Dify Cloud restricts external API calls via SSRF proxy. "
                    "actionbook.dev may not be whitelisted. "
                    "Recommendation: Use Dify Self-hosted or request whitelisting from Dify support."
                )
        except requests.Timeout:
            logger.exception("Timeout calling Actionbook API")
            yield self.create_text_message(
                "Error: Request to Actionbook API timed out after 30 seconds. "
                "This may indicate network restrictions in Dify Cloud. "
                "For unrestricted access, consider using Dify Self-hosted."
            )
        except Exception as e:
            logger.exception("Unexpected error in search_actions")
            print(f"[search_actions] Exception type={type(e).__name__}, message={e}", file=sys.stderr, flush=True)
            yield self.create_text_message(
                f"Error: An unexpected error occurred ({type(e).__name__}: {e}). "
                "Please check plugin logs for details."
            )
        except BaseException as e:
            # Catch gevent.Timeout and other non-Exception errors
            logger.critical(f"BaseException in search_actions: {type(e).__name__}: {e}")
            print(f"[search_actions] BaseException type={type(e).__name__}, message={e}", file=sys.stderr, flush=True)
            yield self.create_text_message(
                f"Error: A system-level error occurred ({type(e).__name__}: {e}). "
                "This may indicate network restrictions or timeout in Dify Cloud environment. "
                "Consider using Dify Self-hosted for unrestricted access."
            )
            # DO NOT raise - let generator finish to ensure message is delivered to user

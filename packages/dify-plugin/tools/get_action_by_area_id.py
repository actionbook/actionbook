"""Get Action By Area ID Tool - Retrieve full action details."""

import logging
import sys
from collections.abc import Generator
from typing import Any

import requests
from dify_plugin import Tool
from dify_plugin.entities.tool import ToolInvokeMessage

from constants import API_BASE_URL

logger = logging.getLogger(__name__)


class GetActionByAreaIdTool(Tool):
    """Retrieve complete action details by area ID."""

    def _invoke(self, tool_parameters: dict[str, Any]) -> Generator[ToolInvokeMessage, None, None]:
        """
        Fetch action details by area ID.

        Args:
            tool_parameters: Dict with keys:
                - area_id (required): Area ID in format "site:path:area"

        Yields:
            ToolInvokeMessage with full action details as formatted text
        """
        print(f"[get_action_by_area_id] _invoke called with parameters: {tool_parameters}", file=sys.stderr, flush=True)

        try:
            area_id = tool_parameters.get("area_id", "").strip() if tool_parameters.get("area_id") else ""

            if not area_id:
                yield self.create_text_message("Error: 'area_id' parameter is required.")
                return

            parts = area_id.split(":")
            if len(parts) < 3 or any(not part.strip() for part in parts[:3]):
                yield self.create_text_message(
                    f"Error: Invalid area_id format. Expected 'site:path:area' "
                    f"(e.g., 'github.com:login:email-input'), got: {area_id}"
                )
                return

            headers = {"Accept": "text/plain"}

            print(f"[get_action_by_area_id] Making request to {API_BASE_URL}/api/get_action_by_area_id with area_id={area_id}", file=sys.stderr, flush=True)

            response = requests.get(
                f"{API_BASE_URL}/api/get_action_by_area_id",
                headers=headers,
                params={"area_id": area_id},
                timeout=30,
            )

            print(f"[get_action_by_area_id] Response status={response.status_code}", file=sys.stderr, flush=True)

            if response.status_code == 404:
                yield self.create_text_message(f"Action not found for area_id: {area_id}")
                return
            elif response.status_code == 401:
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
                    f"Error: Received empty response for area_id: {area_id}. "
                    "This often indicates that Dify Cloud's SSRF proxy is blocking the request. "
                    "actionbook.dev may not be in the whitelist. "
                    "\n\nSolutions:\n"
                    "1. Use Dify Self-hosted (recommended for full control)\n"
                    "2. Contact Dify support to whitelist actionbook.dev"
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
            logger.exception("Unexpected error in get_action_by_area_id")
            print(f"[get_action_by_area_id] Exception type={type(e).__name__}, message={e}", file=sys.stderr, flush=True)
            yield self.create_text_message(
                f"Error: An unexpected error occurred ({type(e).__name__}: {e}). "
                "Please check plugin logs for details."
            )
        except BaseException as e:
            logger.critical(f"BaseException in get_action_by_area_id: {type(e).__name__}: {e}")
            print(f"[get_action_by_area_id] BaseException type={type(e).__name__}, message={e}", file=sys.stderr, flush=True)
            yield self.create_text_message(
                f"Error: A system-level error occurred ({type(e).__name__}: {e}). "
                "This may indicate network restrictions or timeout in Dify Cloud environment. "
                "Consider using Dify Self-hosted for unrestricted access."
            )
            # DO NOT raise - let generator finish to ensure message is delivered to user

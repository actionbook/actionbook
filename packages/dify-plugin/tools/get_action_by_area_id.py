"""Get Action By Area ID Tool - Retrieve full action details."""

import logging
import re
from collections.abc import Generator
from typing import Any

import requests
from dify_plugin import Tool
from dify_plugin.entities.tool import ToolInvokeMessage

from constants import API_BASE_URL
from utils.http_guard import build_html_misroute_message, looks_like_html_response

_AREA_ID_PART_RE = re.compile(r"^[a-zA-Z0-9._\-/]+$")

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
            if not all(_AREA_ID_PART_RE.match(p.strip()) for p in parts[:3]):
                yield self.create_text_message(
                    "Error: area_id contains invalid characters. "
                    "Only alphanumeric, dots, hyphens, underscores, and slashes are allowed."
                )
                return

            headers = {"Accept": "text/plain"}
            request_url = f"{API_BASE_URL}/api/get_action_by_area_id"

            response = requests.get(
                request_url,
                headers=headers,
                params={"area_id": area_id},
                timeout=30,
            )
            result_text = response.text or ""

            if looks_like_html_response(response, result_text):
                yield self.create_text_message(
                    build_html_misroute_message(API_BASE_URL, request_url)
                )
                return

            if response.status_code == 404:
                yield self.create_text_message(
                    f"Action not found for area_id: {area_id}"
                )
                return
            elif response.status_code == 401:
                yield self.create_text_message(
                    "Error: Unauthorized (401). Please check your API credentials."
                )
                return
            elif response.status_code == 429:
                yield self.create_text_message(
                    "Error: Rate limit exceeded (429). Please retry after a short wait."
                )
                return
            elif response.status_code >= 500:
                yield self.create_text_message(
                    f"Error: Server error ({response.status_code}). Please try again later."
                )
                return
            elif response.status_code != 200:
                yield self.create_text_message(
                    f"Error: Unexpected status {response.status_code} from {request_url}."
                )
                return

            if not result_text or result_text.strip() == "":
                yield self.create_text_message(
                    f"Error: Received empty response for area_id: {area_id}.\n\n"
                    "This likely indicates Dify Cloud's SSRF proxy is intercepting the request.\n\n"
                    "Solutions:\n"
                    "1. Use Dify Self-hosted\n"
                    "2. Contact Dify support to whitelist api.actionbook.dev"
                )
            else:
                yield self.create_text_message(result_text)

        except requests.ConnectionError as e:
            logger.exception("Connection error calling Actionbook API")
            yield self.create_text_message(
                f"Error: Could not connect to {API_BASE_URL}.\n"
                f"{type(e).__name__}: {e}"
            )
        except requests.Timeout:
            logger.exception("Timeout calling Actionbook API")
            yield self.create_text_message(
                f"Error: Request to {API_BASE_URL} timed out after 30s."
            )
        except Exception as e:
            logger.exception("Unexpected error in get_action_by_area_id")
            yield self.create_text_message(
                f"Error: {type(e).__name__}: {e}"
            )

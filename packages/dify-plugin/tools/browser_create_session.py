"""Browser Create Session Tool — start a managed cloud browser session."""

import json
import logging
from collections.abc import Generator
from typing import Any

from dify_plugin import Tool
from dify_plugin.entities.tool import ToolInvokeMessage

from providers import get_provider

logger = logging.getLogger(__name__)


class BrowserCreateSessionTool(Tool):
    """Create a cloud browser session via a managed provider."""

    def _invoke(self, tool_parameters: dict[str, Any]) -> Generator[ToolInvokeMessage, None, None]:
        provider_name = (tool_parameters.get("provider") or "hyperbrowser").strip()
        api_key = (tool_parameters.get("api_key") or "").strip()
        profile_id = (tool_parameters.get("profile_id") or "").strip() or None
        use_proxy = str(tool_parameters.get("use_proxy", "false")).lower().strip() == "true"

        if not api_key:
            yield self.create_text_message("Error: 'api_key' is required.")
            return

        try:
            provider = get_provider(provider_name, api_key)
            session = provider.create_session(
                profile_id=profile_id,
                use_proxy=use_proxy,
            )

            result = {
                "ws_endpoint": session.ws_endpoint,
                "session_id": session.session_id,
                "provider": provider_name,
            }

            yield self.create_text_message(
                f"Browser session created.\n"
                f"Provider:          {provider_name}\n"
                f"Session ID:        {session.session_id}\n"
                f"WebSocket Endpoint: {session.ws_endpoint}\n\n"
                f"Pass `ws_endpoint` as the `cdp_url` parameter to browser tools.\n"
                f"Pass `session_id` to browser_stop_session when done.\n\n"
                f"```json\n{json.dumps(result, indent=2)}\n```"
            )

        except NotImplementedError as e:
            yield self.create_text_message(f"Error: Provider not yet implemented. {e}")
        except ValueError as e:
            yield self.create_text_message(f"Error: {e}")
        except Exception as e:
            logger.exception("Failed to create browser session with provider '%s'", provider_name)
            yield self.create_text_message(
                f"Error: Failed to create session with '{provider_name}': "
                f"{type(e).__name__}: {e}"
            )

"""Browser Stop Session Tool — release a managed cloud browser session."""

import logging
from collections.abc import Generator
from typing import Any

from dify_plugin import Tool
from dify_plugin.entities.tool import ToolInvokeMessage

from providers import get_provider

logger = logging.getLogger(__name__)


class BrowserStopSessionTool(Tool):
    """Stop a cloud browser session and persist profile state."""

    def _invoke(self, tool_parameters: dict[str, Any]) -> Generator[ToolInvokeMessage, None, None]:
        provider_name = (tool_parameters.get("provider") or "hyperbrowser").strip()
        api_key = (tool_parameters.get("api_key") or "").strip()
        session_id = (tool_parameters.get("session_id") or "").strip()

        if not api_key:
            yield self.create_text_message("Error: 'api_key' is required.")
            return

        if not session_id:
            yield self.create_text_message("Error: 'session_id' is required.")
            return

        try:
            provider = get_provider(provider_name, api_key)
            provider.stop_session(session_id)
            yield self.create_text_message(
                f"Session stopped.\n"
                f"Provider:   {provider_name}\n"
                f"Session ID: {session_id}\n\n"
                "Profile state has been persisted (if a profile_id was used)."
            )

        except NotImplementedError as e:
            yield self.create_text_message(f"Error: Provider not yet implemented. {e}")
        except ValueError as e:
            yield self.create_text_message(f"Error: {e}")
        except Exception as e:
            logger.exception(
                "Failed to stop session '%s' on provider '%s'",
                session_id,
                provider_name,
            )
            yield self.create_text_message(
                f"Error: Failed to stop session '{session_id}' on '{provider_name}': "
                f"{type(e).__name__}: {e}"
            )

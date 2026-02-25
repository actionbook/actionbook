"""Browser Stop Session Tool — release a managed cloud browser session."""

import logging
import threading
from collections.abc import Generator
from typing import Any

from dify_plugin import Tool
from dify_plugin.entities.tool import ToolInvokeMessage

from providers import get_provider

# Lazy import — avoid loading connection_pool at module level.
# connection_pool spawns a background cleanup thread on import,
# which may fail in Dify Cloud's serverless runtime.
pool = None  # Module-level alias set by _ensure_pool(); patchable by tests.
_pool = None
_pool_lock = threading.Lock()


def _ensure_pool():
    """Thread-safe lazy import of connection pool."""
    global pool, _pool
    if _pool is not None:
        return
    with _pool_lock:
        if _pool is not None:
            return
        from utils.connection_pool import pool as _p
        _pool = _p
        pool = _p

logger = logging.getLogger(__name__)


class BrowserStopSessionTool(Tool):
    """Stop a cloud browser session and persist profile state."""

    def _invoke(self, tool_parameters: dict[str, Any]) -> Generator[ToolInvokeMessage, None, None]:
        if pool is None:
            try:
                _ensure_pool()
            except Exception as e:
                yield self.create_text_message(
                    f"Error: Browser pool unavailable: {type(e).__name__}: {e}\n"
                    "This environment may not support browser tools."
                )
                return

        session_id = (tool_parameters.get("session_id") or "").strip()

        if not session_id:
            yield self.create_text_message("Error: 'session_id' is required.")
            return

        # Look up provider/api_key cached when the session was created
        session_info = pool.get_session_info(session_id)
        if session_info is None:
            yield self.create_text_message(
                f"Error: No active session found for session_id '{session_id}'.\n"
                "Was browser_create_session called first in this plugin process?"
            )
            return

        provider_name, api_key = session_info

        try:
            provider = get_provider(provider_name, api_key)
            provider.stop_session(session_id)

        except NotImplementedError as e:
            yield self.create_text_message(f"Error: Provider not yet implemented. {e}")
            return
        except ValueError as e:
            yield self.create_text_message(f"Error: {e}")
            return
        except Exception as e:
            logger.error("Failed to stop browser session.")
            yield self.create_text_message(
                f"Error: Failed to stop browser session.\n"
                f"Reason: {type(e).__name__}: {e}\n"
                f"Verify your API key is valid and the provider service is reachable."
            )
            return

        try:
            pool.disconnect(session_id)
        except Exception:
            logger.warning("Failed to disconnect local session after remote stop.")

        yield self.create_text_message(
            f"Session stopped.\n"
            f"Provider:   {provider_name}\n"
            f"Session ID: {session_id}\n\n"
            "Profile state has been persisted (if a profile_id was used)."
        )

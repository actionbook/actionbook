"""Browser Create Session Tool — start a managed cloud browser session."""

import json
import logging
from collections.abc import Generator
from typing import Any

from dify_plugin import Tool
from dify_plugin.entities.tool import ToolInvokeMessage

from providers import SUPPORTED_PROVIDERS, get_provider
from utils.api_key import resolve_provider_api_key
from utils.connection_pool import pool

logger = logging.getLogger(__name__)


class BrowserCreateSessionTool(Tool):
    """Create a cloud browser session via a managed provider."""

    def _invoke(self, tool_parameters: dict[str, Any]) -> Generator[ToolInvokeMessage, None, None]:
        provider_name = (tool_parameters.get("provider") or "hyperbrowser").strip()
        if provider_name not in SUPPORTED_PROVIDERS:
            yield self.create_text_message(
                f"Error: Unknown provider '{provider_name}'. "
                f"Supported: {', '.join(sorted(SUPPORTED_PROVIDERS))}"
            )
            return

        api_key = resolve_provider_api_key(tool_parameters.get("api_key") or "")
        profile_id = (tool_parameters.get("profile_id") or "").strip() or None
        use_proxy = str(tool_parameters.get("use_proxy", "false")).lower().strip() == "true"

        if not api_key:
            yield self.create_text_message(
                "Error: 'api_key' is required. "
                "Get your key at https://app.hyperbrowser.ai/"
            )
            return

        try:
            provider = get_provider(provider_name, api_key)
            profile_fallback_note = ""
            session = None

            try:
                session = provider.create_session(
                    profile_id=profile_id,
                    use_proxy=use_proxy,
                )
            except Exception as create_err:
                err_msg = str(create_err).lower()

                # profile_id not accepted -> retry without profile persistence.
                if profile_id and (
                    "profile" in err_msg and (
                        "invalid uuid" in err_msg
                        or "profile not found" in err_msg
                    )
                ):
                    logger.warning(
                        "Create session with profile_id failed (%s). Retrying without profile_id.",
                        create_err,
                    )
                    session = provider.create_session(
                        profile_id=None,
                        use_proxy=use_proxy,
                    )
                    profile_fallback_note = (
                        "Note: profile_id was not accepted by provider; "
                        "session created without profile persistence.\n"
                    )
                else:
                    raise

            if session is None:
                raise RuntimeError("Failed to create session after retries.")

            # Cache CDP connection in the pool for multi-step reuse
            try:
                pool.connect(
                    session.session_id,
                    session.ws_endpoint,
                    provider_name=provider_name,
                    api_key=api_key,
                )
            except Exception as pool_err:
                logger.error("Pool connect failed after remote session creation.")
                cleanup_failed = False
                try:
                    provider.stop_session(session.session_id)
                except Exception:
                    cleanup_failed = True
                    logger.error("Failed to clean up remote session after pool connect failure.")

                if cleanup_failed:
                    yield self.create_text_message(
                        "Error: Failed to initialize local session cache after creating remote session.\n"
                        f"Reason: {type(pool_err).__name__}: {pool_err}\n"
                        "Automatic remote cleanup also failed, so the session may still be running.\n"
                        f"Session ID: {session.session_id}\n"
                        "Please retry browser_stop_session with this session_id to avoid resource leakage."
                    )
                else:
                    yield self.create_text_message(
                        "Error: Failed to initialize local session cache after creating remote session.\n"
                        f"Reason: {type(pool_err).__name__}: {pool_err}\n"
                        "The remote session was closed automatically."
                    )
                return

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
                f"{profile_fallback_note}"
                f"For reliability, pass BOTH `session_id` and `cdp_url` "
                f"(this ws_endpoint) to browser_operator calls.\n"
                f"Pass `session_id` to browser_stop_session when done.\n\n"
                f"```json\n{json.dumps(result, indent=2)}\n```"
            )

        except NotImplementedError as e:
            yield self.create_text_message(f"Error: Provider not yet implemented. {e}")
        except ValueError as e:
            yield self.create_text_message(f"Error: {e}")
        except Exception as e:
            logger.error("Failed to create browser session.")
            yield self.create_text_message(
                f"Error: Failed to create browser session.\n"
                f"Reason: {type(e).__name__}: {e}\n"
                f"Verify your API key is valid and the provider service is reachable."
            )

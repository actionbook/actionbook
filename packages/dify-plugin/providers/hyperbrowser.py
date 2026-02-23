"""Hyperbrowser cloud browser provider.

Persistence model: Profile-based.
Each Dify tool call creates a NEW session but loads the same Profile,
restoring cookies / localStorage from the previous call.
Only active session time is billed — idle waiting between calls is free.

CDP connection: ws_endpoint is returned directly by the SDK.
No additional SDK calls needed after create_session().

Docs: https://docs.hyperbrowser.ai/sessions/profiles
"""

import logging
import uuid
from dataclasses import dataclass
from typing import Any

# Module-level conditional import so tests can patch providers.hyperbrowser.Hyperbrowser
# and providers.hyperbrowser.CreateSessionParams.
try:
    from hyperbrowser import Hyperbrowser
    from hyperbrowser.models import CreateSessionParams
except ImportError:
    Hyperbrowser = None  # type: ignore[assignment, misc]
    CreateSessionParams = None  # type: ignore[assignment, misc]

logger = logging.getLogger(__name__)


@dataclass
class HyperbrowserSession:
    """Active Hyperbrowser session."""

    _ws_endpoint: str
    _session_id: str
    _client: Any  # hyperbrowser.Hyperbrowser instance

    @property
    def ws_endpoint(self) -> str:
        return self._ws_endpoint

    @property
    def session_id(self) -> str:
        return self._session_id

    def stop(self) -> None:
        """Stop session and persist Profile state."""
        try:
            self._client.sessions.stop(self._session_id)
        except Exception:
            logger.exception("Failed to stop Hyperbrowser session %s", self._session_id)
            raise


class HyperbrowserProvider:
    """
    Cloud browser provider backed by Hyperbrowser.

    Session persistence strategy (Dify workflow context):
    - Pass a stable profile_id (e.g., f"dify-{workflow_id}-{user_id}")
    - Set persist_changes=True so cookies/localStorage are saved on stop()
    - Next Dify tool call creates a fresh session but loads the same profile
    - This avoids billing for idle time between Dify HTTP calls

    See: https://docs.hyperbrowser.ai/sessions/profiles
    """

    def __init__(self, api_key: str) -> None:
        if Hyperbrowser is None:
            raise ImportError(
                "hyperbrowser package is not installed. "
                "Add 'hyperbrowser>=0.1.0' to pyproject.toml dependencies."
            )
        self._client = Hyperbrowser(api_key=api_key)

    def create_session(
        self,
        profile_id: str | None = None,
        use_proxy: bool = False,
        persist_changes: bool = True,
        **kwargs: Any,
    ) -> HyperbrowserSession:
        """
        Create a Hyperbrowser session.

        Args:
            profile_id:      Profile ID for persistent state. Recommended for
                             workflows that need to maintain login across tool calls.
                             Example: "dify-user-abc123"
            use_proxy:       Route through a residential proxy (helps bypass
                             geo-restrictions and bot detection).
            persist_changes: Whether to save browser state to the Profile when the
                             session is stopped. Only relevant when profile_id is set.
        """
        params_kwargs: dict[str, Any] = {"use_proxy": use_proxy}

        if profile_id:
            normalized_profile_id = _normalize_profile_id(profile_id)
            params_kwargs["profile"] = {
                "id": normalized_profile_id,
                "persist_changes": persist_changes,
            }

        params = CreateSessionParams(**params_kwargs)
        session = self._client.sessions.create(params=params)

        return HyperbrowserSession(
            _ws_endpoint=session.ws_endpoint,
            _session_id=session.id,
            _client=self._client,
        )

    def stop_session(self, session_id: str) -> None:
        """Stop session by ID. Profile state is persisted on stop."""
        self._client.sessions.stop(session_id)


def _normalize_profile_id(profile_id: str) -> str:
    """Normalize arbitrary profile_id input to UUID string accepted by Hyperbrowser."""
    raw = profile_id.strip()
    if not raw:
        raise ValueError("profile_id cannot be empty when provided")

    try:
        return str(uuid.UUID(raw))
    except ValueError:
        # Keep deterministic mapping so repeated runs reuse the same profile.
        return str(uuid.uuid5(uuid.NAMESPACE_URL, f"actionbook:{raw}"))

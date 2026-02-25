"""Hyperbrowser cloud browser provider.

Persistence model: Profile-based.
Each Dify tool call creates a NEW session but loads the same Profile,
restoring cookies / localStorage from the previous call.
Only active session time is billed — idle waiting between calls is free.

CDP connection: ws_endpoint is returned directly by the API.
No additional SDK calls needed after create_session().

Uses direct HTTP API calls instead of the hyperbrowser SDK to avoid
extra dependencies in Dify Cloud's serverless runtime.

Docs: https://docs.hyperbrowser.ai/sessions/profiles
API:  https://docs.hyperbrowser.ai/api-reference
"""

import logging
import uuid
from dataclasses import dataclass
from typing import Any

import requests

logger = logging.getLogger(__name__)

_API_BASE = "https://api.hyperbrowser.ai"
_TIMEOUT = 30


@dataclass
class HyperbrowserSession:
    """Active Hyperbrowser session."""

    _ws_endpoint: str
    _session_id: str
    _api_key: str

    @property
    def ws_endpoint(self) -> str:
        return self._ws_endpoint

    @property
    def session_id(self) -> str:
        return self._session_id

    def stop(self) -> None:
        """Stop session and persist Profile state."""
        try:
            resp = requests.put(
                f"{_API_BASE}/api/session/{self._session_id}/stop",
                headers={"x-api-key": self._api_key},
                timeout=_TIMEOUT,
            )
            resp.raise_for_status()
        except Exception:
            logger.exception("Failed to stop Hyperbrowser session %s", self._session_id)
            raise


class HyperbrowserProvider:
    """
    Cloud browser provider backed by Hyperbrowser REST API.

    Session persistence strategy (Dify workflow context):
    - Pass a stable profile_id (e.g., f"dify-{workflow_id}-{user_id}")
    - Set persist_changes=True so cookies/localStorage are saved on stop()
    - Next Dify tool call creates a fresh session but loads the same profile
    - This avoids billing for idle time between Dify HTTP calls

    See: https://docs.hyperbrowser.ai/sessions/profiles
    """

    def __init__(self, api_key: str) -> None:
        self._api_key = api_key

    def create_session(
        self,
        profile_id: str | None = None,
        use_proxy: bool = False,
        persist_changes: bool = True,
        **kwargs: Any,
    ) -> HyperbrowserSession:
        """
        Create a Hyperbrowser session via REST API.

        Args:
            profile_id:      Profile ID for persistent state.
            use_proxy:       Route through a residential proxy.
            persist_changes: Save browser state to Profile on stop.
        """
        body: dict[str, Any] = {"useProxy": use_proxy}

        if profile_id:
            normalized_profile_id = _normalize_profile_id(profile_id)
            body["profile"] = {
                "id": normalized_profile_id,
                "persistChanges": persist_changes,
            }

        url = f"{_API_BASE}/api/session"
        resp = requests.post(
            url,
            headers={
                "x-api-key": self._api_key,
                "Content-Type": "application/json",
            },
            json=body,
            timeout=_TIMEOUT,
        )

        if not resp.ok:
            body_preview = resp.text[:300] if resp.text else "(empty)"
            raise RuntimeError(
                f"Hyperbrowser API error: HTTP {resp.status_code}\n"
                f"Response: {body_preview}"
            )

        data = resp.json()

        session_id = data.get("id") or data.get("sessionId", "")
        ws_endpoint = data.get("wsEndpoint") or data.get("sessionWebsocketUrl", "")

        if not session_id or not ws_endpoint:
            raise RuntimeError(
                f"Hyperbrowser API returned incomplete session data: {data}"
            )

        return HyperbrowserSession(
            _ws_endpoint=ws_endpoint,
            _session_id=session_id,
            _api_key=self._api_key,
        )

    def stop_session(self, session_id: str) -> None:
        """Stop session by ID. Profile state is persisted on stop."""
        resp = requests.put(
            f"{_API_BASE}/api/session/{session_id}/stop",
            headers={"x-api-key": self._api_key},
            timeout=_TIMEOUT,
        )
        resp.raise_for_status()


def _normalize_profile_id(profile_id: str) -> str:
    """Normalize arbitrary profile_id input to UUID string accepted by Hyperbrowser."""
    raw = profile_id.strip()
    if not raw:
        raise ValueError("profile_id cannot be empty when provided")

    try:
        return str(uuid.UUID(raw))
    except ValueError:
        return str(uuid.uuid5(uuid.NAMESPACE_URL, f"actionbook:{raw}"))

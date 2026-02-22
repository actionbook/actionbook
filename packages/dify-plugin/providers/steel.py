"""Steel.dev cloud browser provider — FUTURE IMPLEMENTATION.

Not yet implemented. Tracked in: https://steel.dev

When implementing, key differences from Hyperbrowser:
- Session persistence: Fixed session_id (reconnect to same session)
  wss://connect.steel.dev?apiKey={KEY}&sessionId={UUID}
- Billing: Session alive time (including idle) — not Profile-based
- Free tier: Hobby $10 credits/month (~100hr), 5 concurrent sessions
- Session max duration: Developer plan ($99/mo) needed for >30 min sessions
- Self-hostable: https://github.com/steel-dev/steel-browser

Implementation sketch:
    class SteelProvider:
        def __init__(self, api_key):
            from steel import Steel
            self._client = Steel(steel_api_key=api_key)
            self._api_key = api_key

        def create_session(self, profile_id=None, **kwargs):
            # profile_id maps to session_id for reconnect
            session = self._client.sessions.create(
                session_id=profile_id,  # fixed UUID → deterministic reconnect
                timeout=3600000,        # 1hr (requires Developer plan)
            )
            ws_url = (
                f"wss://connect.steel.dev"
                f"?apiKey={self._api_key}"
                f"&sessionId={session.id}"
            )
            return SteelSession(ws_url, session.id, self._client)

        def stop_session(self, session_id):
            self._client.sessions.release(session_id)
"""

from typing import Any


class SteelProvider:
    """Steel.dev provider — not yet implemented."""

    def __init__(self, api_key: str) -> None:
        raise NotImplementedError(
            "Steel.dev provider is not yet implemented. "
            "Use provider='hyperbrowser' instead.\n"
            "See implementation guide in this file's module docstring."
        )

    def create_session(self, profile_id: str | None = None, **kwargs: Any) -> None:
        raise NotImplementedError("Steel.dev provider not yet implemented.")

    def stop_session(self, session_id: str) -> None:
        raise NotImplementedError("Steel.dev provider not yet implemented.")

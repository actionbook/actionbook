"""Browser provider protocol definitions.

New providers must implement BrowserSession and BrowserProvider.
No inheritance required — structural subtyping (Protocol) is used.

Implementing a new provider:
1. Create providers/{name}.py
2. Implement BrowserSession and BrowserProvider classes
3. Register the provider in providers/__init__.py
"""

from typing import Any, Protocol, runtime_checkable


@runtime_checkable
class BrowserSession(Protocol):
    """Represents an active cloud browser session."""

    @property
    def ws_endpoint(self) -> str:
        """WebSocket CDP endpoint for Playwright connect_over_cdp()."""
        ...

    @property
    def session_id(self) -> str:
        """Provider-specific session identifier for stop_session()."""
        ...

    def stop(self) -> None:
        """Release session resources on the provider side."""
        ...


@runtime_checkable
class BrowserProvider(Protocol):
    """Interface for cloud browser providers."""

    def create_session(
        self,
        profile_id: str | None = None,
        **kwargs: Any,
    ) -> BrowserSession:
        """
        Create a new browser session.

        Args:
            profile_id: Optional identifier for persistent browser state.
                        On Hyperbrowser this maps to a Profile.
                        On Steel this maps to a fixed session_id.
                        Pass None for a stateless ephemeral session.
            **kwargs:   Provider-specific options (e.g., use_proxy).

        Returns:
            BrowserSession with ws_endpoint ready for connect_over_cdp().
        """
        ...

    def stop_session(self, session_id: str) -> None:
        """
        Stop a session by its provider-specific ID.

        Args:
            session_id: Value from BrowserSession.session_id.
        """
        ...

"""Actionbook Dify Plugin - Tool Provider Implementation."""

from typing import Any

from dify_plugin import ToolProvider


class ActionbookProvider(ToolProvider):
    """Manages tool instantiation for Actionbook."""

    def _validate_credentials(self, credentials: dict[str, Any]) -> None:
        """Validate provider credentials (currently a no-op — public API access)."""
        return

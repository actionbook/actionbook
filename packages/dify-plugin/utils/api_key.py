"""Shared API key resolution for cloud browser providers."""

import os

_API_KEY_PLACEHOLDERS = frozenset({
    "",
    "key",
    "api_key",
    "your_api_key",
    "your_key",
    "example_api_key",
    "example_key",
    "test_key",
    "replace_with_valid_apikey",
    "replace_with_valid_api_key",
})


def resolve_provider_api_key(raw_key: str) -> str:
    """Resolve a provider API key from user input or environment variables.

    Priority:
    1. User-supplied key (if not a known placeholder)
    2. HYPERBROWSER_API_KEY environment variable
    3. ACTIONBOOK_HYPERBROWSER_API_KEY environment variable
    4. Empty string (caller must handle)

    Args:
        raw_key: Raw key string from tool parameters.

    Returns:
        Resolved API key, or empty string if none available.
    """
    key = (raw_key or "").strip()
    if key and key.lower() not in _API_KEY_PLACEHOLDERS:
        return key

    env_key = (
        os.environ.get("HYPERBROWSER_API_KEY")
        or os.environ.get("ACTIONBOOK_HYPERBROWSER_API_KEY")
        or ""
    ).strip()
    return env_key

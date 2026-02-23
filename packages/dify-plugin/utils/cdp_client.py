"""CDP URL validation utility."""

import logging
from urllib.parse import urlparse

logger = logging.getLogger(__name__)

_VALID_CDP_PREFIXES = ("ws://", "wss://", "http://", "https://")


def validate_cdp_url(cdp_url: str) -> str:
    """Validate and normalise a CDP URL string.

    Returns the stripped URL on success. Raises ValueError on bad input.
    """
    url = cdp_url.strip() if cdp_url else ""

    if not url:
        raise ValueError("'cdp_url' is required and cannot be empty")

    if not any(url.startswith(p) for p in _VALID_CDP_PREFIXES):
        raise ValueError(
            f"Invalid cdp_url: '{url}'. "
            "Must start with ws://, wss://, http://, or https://"
        )

    # Warn when using insecure schemes with non-localhost hosts
    if url.startswith(("ws://", "http://")):
        parsed = urlparse(url)
        host = (parsed.hostname or "").lower()
        if host not in ("localhost", "127.0.0.1", "::1"):
            logger.warning(
                "Insecure CDP connection to non-localhost host. "
                "Consider using wss:// or https:// for remote connections."
            )

    return url

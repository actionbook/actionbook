"""Fixtures for E2E browser tests requiring a real Hyperbrowser session.

Retry logic handles intermittent SSL failures caused by gevent's
monkey.patch_all(sys=True) in dify_plugin.__init__.
"""

import json
import logging
import os
import ssl
import time
from pathlib import Path

import pytest
from dotenv import load_dotenv

logger = logging.getLogger(__name__)

# Load .env from the dify-plugin package root
_pkg_root = Path(__file__).parent.parent.parent
load_dotenv(_pkg_root / ".env")

# Retry config for gevent SSL flakiness
MAX_RETRIES = 3
RETRY_DELAY = 2  # seconds


def _is_ssl_error(exc: Exception) -> bool:
    """Check if an exception is caused by gevent SSL monkey-patching."""
    msg = str(exc).lower()
    return any(pattern in msg for pattern in [
        "ssl",
        "unexpected_eof",
        "tls connection",
        "network socket disconnected",
        "connection reset",
    ])


def _create_session_with_retry(api_key: str, retries: int = MAX_RETRIES):
    """Create a Hyperbrowser session with retry for SSL errors."""
    from hyperbrowser import Hyperbrowser
    from hyperbrowser.models import CreateSessionParams

    last_exc = None
    for attempt in range(1, retries + 1):
        try:
            client = Hyperbrowser(api_key=api_key)
            session = client.sessions.create(params=CreateSessionParams(use_proxy=False))
            return client, session
        except (ssl.SSLError, OSError, ConnectionError) as exc:
            last_exc = exc
            if attempt < retries and _is_ssl_error(exc):
                logger.warning(
                    "SSL error on session creation (attempt %d/%d): %s — retrying in %ds",
                    attempt, retries, exc, RETRY_DELAY,
                )
                time.sleep(RETRY_DELAY)
            else:
                raise
        except Exception as exc:
            if _is_ssl_error(exc):
                last_exc = exc
                if attempt < retries:
                    logger.warning(
                        "SSL error on session creation (attempt %d/%d): %s — retrying in %ds",
                        attempt, retries, exc, RETRY_DELAY,
                    )
                    time.sleep(RETRY_DELAY)
                else:
                    raise
            else:
                raise
    raise last_exc  # type: ignore[misc]


@pytest.fixture()
def hyperbrowser_api_key():
    """Read HYPERBROWSER_API_KEY from env; skip if unset."""
    key = os.environ.get("HYPERBROWSER_API_KEY")
    if not key:
        pytest.skip("HYPERBROWSER_API_KEY not set — skipping E2E browser test")
    return key


@pytest.fixture()
def hyperbrowser_session(hyperbrowser_api_key):
    """Create a Hyperbrowser session, yield (api_key, ws_endpoint, session_id), stop on exit.

    Uses the SDK directly (not the Dify tool) to avoid gevent SSL conflicts
    and to provide a clean session for each test. Retries on SSL errors.
    """
    client, session = _create_session_with_retry(hyperbrowser_api_key)

    yield {
        "api_key": hyperbrowser_api_key,
        "ws_endpoint": session.ws_endpoint,
        "session_id": session.id,
        "client": client,
    }

    # Always clean up
    try:
        client.sessions.stop(session.id)
    except Exception:
        pass


def parse_session_output(text: str) -> dict:
    """Extract the JSON block from browser_create_session tool output."""
    json_str = text.split("```json\n")[1].split("\n```")[0]
    return json.loads(json_str)

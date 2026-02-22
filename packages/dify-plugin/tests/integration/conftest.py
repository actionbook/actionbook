"""Fixtures for integration tests that hit the real Actionbook API."""

import pytest
import requests


@pytest.fixture()
def api_headers(api_key):
    """Standard request headers for Actionbook API."""
    return {
        "X-API-Key": api_key,
        "Accept": "text/plain",
    }


@pytest.fixture()
def api_session(api_headers):
    """Pre-configured requests.Session with auth headers."""
    session = requests.Session()
    session.headers.update(api_headers)
    return session

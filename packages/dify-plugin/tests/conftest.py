"""Shared fixtures and markers for Actionbook Dify Plugin tests."""

import os

import pytest


@pytest.fixture()
def api_key():
    """Read ACTIONBOOK_API_KEY from env; skip if unset."""
    key = os.environ.get("ACTIONBOOK_API_KEY")
    if not key:
        pytest.skip("ACTIONBOOK_API_KEY not set – skipping integration test")
    return key


@pytest.fixture()
def api_base_url():
    """Actionbook API base URL (overridable via env)."""
    return os.environ.get("ACTIONBOOK_API_BASE_URL", "https://api.actionbook.dev")

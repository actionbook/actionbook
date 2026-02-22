"""Layer 2 – API connectivity and contract tests against the real Actionbook API.

Every test in this module is marked ``@pytest.mark.integration`` so that
``uv run pytest`` (default) skips them.  Run with:

    uv run pytest -m integration -v --timeout=60 --no-cov
"""

import uuid

import pytest
import requests

from _plugin import ActionbookProvider

pytestmark = pytest.mark.integration


# ---------------------------------------------------------------------------
# Connectivity
# ---------------------------------------------------------------------------


class TestConnectivity:
    """Basic reachability of Actionbook API endpoints."""

    def test_search_endpoint_reachable(self, api_headers, api_base_url):
        """GET /api/search_actions returns 200 with non-empty text."""
        resp = requests.get(
            f"{api_base_url}/api/search_actions",
            headers=api_headers,
            params={"query": "login", "page_size": 3},
            timeout=30,
        )
        assert resp.status_code == 200
        assert resp.text.strip() != ""

    def test_get_action_endpoint_reachable(self, api_headers, api_base_url):
        """GET /api/get_action_by_area_id returns 200 or 404 (not 5xx)."""
        resp = requests.get(
            f"{api_base_url}/api/get_action_by_area_id",
            headers=api_headers,
            params={"area_id": "github.com:login:username-field"},
            timeout=30,
        )
        assert resp.status_code in (200, 404)


# ---------------------------------------------------------------------------
# Authentication
# ---------------------------------------------------------------------------


class TestAuthentication:
    """Verify that authentication is enforced correctly."""

    def test_invalid_key_returns_401(self, api_base_url):
        """A bad API key must yield HTTP 401."""
        resp = requests.get(
            f"{api_base_url}/api/search_actions",
            headers={"X-API-Key": "invalid-key-000", "Accept": "text/plain"},
            params={"query": "test", "page_size": 1},
            timeout=15,
        )
        assert resp.status_code == 401

    def test_missing_key_returns_401_or_403(self, api_base_url):
        """No API key header at all should be rejected."""
        resp = requests.get(
            f"{api_base_url}/api/search_actions",
            headers={"Accept": "text/plain"},
            params={"query": "test", "page_size": 1},
            timeout=15,
        )
        assert resp.status_code in (401, 403)

    def test_bearer_token_does_not_work(self, api_key, api_base_url):
        """Using 'Authorization: Bearer' instead of 'X-API-Key' must fail.

        This proves the docs-bug that was fixed: the old header format is
        rejected by the real API.
        """
        resp = requests.get(
            f"{api_base_url}/api/search_actions",
            headers={
                "Authorization": f"Bearer {api_key}",
                "Accept": "text/plain",
            },
            params={"query": "test", "page_size": 1},
            timeout=15,
        )
        assert resp.status_code == 401


# ---------------------------------------------------------------------------
# API contract
# ---------------------------------------------------------------------------


class TestApiContract:
    """Verify response shapes and edge-case handling."""

    def test_search_returns_text_plain(self, api_headers, api_base_url):
        """search_actions should return non-empty text/plain content."""
        resp = requests.get(
            f"{api_base_url}/api/search_actions",
            headers=api_headers,
            params={"query": "login", "page_size": 3},
            timeout=30,
        )
        assert resp.status_code == 200
        assert len(resp.text.strip()) > 0

    def test_domain_filter_accepted(self, api_headers, api_base_url):
        """Passing a ``domain`` param should not cause an error."""
        resp = requests.get(
            f"{api_base_url}/api/search_actions",
            headers=api_headers,
            params={"query": "login", "domain": "github.com", "page_size": 3},
            timeout=30,
        )
        assert resp.status_code == 200

    def test_nonsense_query_returns_200(self, api_headers, api_base_url):
        """A query with no real matches should still return 200 (empty ok)."""
        nonsense = f"zzz_no_match_{uuid.uuid4().hex[:8]}"
        resp = requests.get(
            f"{api_base_url}/api/search_actions",
            headers=api_headers,
            params={"query": nonsense, "page_size": 1},
            timeout=30,
        )
        assert resp.status_code == 200

    def test_nonexistent_area_id_returns_404(self, api_headers, api_base_url):
        """Looking up an area_id that does not exist should return 404."""
        fake = f"fake-{uuid.uuid4().hex[:8]}.com:nonexist:element"
        resp = requests.get(
            f"{api_base_url}/api/get_action_by_area_id",
            headers=api_headers,
            params={"area_id": fake},
            timeout=30,
        )
        assert resp.status_code == 404

    def test_special_char_area_id_no_routing_error(self, api_headers, api_base_url):
        """area_id with special chars should not blow up the router."""
        weird = "example.com:path/with/slash:element with spaces"
        resp = requests.get(
            f"{api_base_url}/api/get_action_by_area_id",
            headers=api_headers,
            params={"area_id": weird},
            timeout=30,
        )
        assert resp.status_code in (200, 404)


# ---------------------------------------------------------------------------
# Provider-level validation (uses real network)
# ---------------------------------------------------------------------------


class TestProviderLive:
    """ActionbookProvider._validate_credentials against the real API."""

    def test_valid_key_passes_validation(self, api_key):
        """A valid API key should not raise."""
        provider = ActionbookProvider()
        provider._validate_credentials({"actionbook_api_key": api_key})

    def test_invalid_key_fails_validation(self):
        """An obviously wrong key should raise ToolProviderCredentialValidationError."""
        from dify_plugin.errors.tool import ToolProviderCredentialValidationError

        provider = ActionbookProvider()
        with pytest.raises(ToolProviderCredentialValidationError):
            provider._validate_credentials({"actionbook_api_key": "bad-key-999"})

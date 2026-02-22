"""Layer 3 – SearchActionsTool integration tests against the real API.

Run with:
    uv run pytest -m integration -v --timeout=60 --no-cov
"""

import uuid

import pytest

from tools.search_actions import SearchActionsTool

pytestmark = pytest.mark.integration


def _make_tool(api_key: str) -> SearchActionsTool:
    return SearchActionsTool.from_credentials({"actionbook_api_key": api_key})


class TestSearchActionsLive:
    """SearchActionsTool with real HTTP calls."""

    def test_basic_search_returns_results(self, api_key):
        """A common keyword like 'login' should return content."""
        tool = _make_tool(api_key)
        results = list(tool._invoke({"query": "login", "limit": 5}))

        assert len(results) == 1
        text = results[0].message.text
        assert len(text.strip()) > 0
        # Should NOT be the "no results" fallback
        assert "No results found" not in text

    def test_search_with_domain_filter(self, api_key):
        """Adding a domain param should not break the request."""
        tool = _make_tool(api_key)
        results = list(
            tool._invoke({"query": "login", "domain": "github.com", "limit": 3})
        )

        assert len(results) == 1
        assert len(results[0].message.text.strip()) > 0

    def test_no_results_search(self, api_key):
        """A random nonsense query should yield the 'no results' message."""
        tool = _make_tool(api_key)
        nonsense = f"zzz_no_match_{uuid.uuid4().hex}"
        results = list(tool._invoke({"query": nonsense, "limit": 5}))

        assert len(results) == 1
        assert "No results found" in results[0].message.text

    # Parameter validation – these do NOT hit the network
    def test_empty_query_raises(self, api_key):
        """Empty query must raise ValueError before any API call."""
        tool = _make_tool(api_key)
        with pytest.raises(ValueError, match="query"):
            list(tool._invoke({"query": "   "}))

    def test_limit_out_of_range_raises(self, api_key):
        """limit=0 and limit=100 must raise ValueError."""
        tool = _make_tool(api_key)
        with pytest.raises(ValueError, match="limit"):
            list(tool._invoke({"query": "test", "limit": 0}))
        with pytest.raises(ValueError, match="50"):
            list(tool._invoke({"query": "test", "limit": 100}))

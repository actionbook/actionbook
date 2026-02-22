"""Layer 3 – GetActionByAreaIdTool integration tests against the real API.

Includes the critical *roundtrip* test: search -> extract area_id -> get details.

Run with:
    uv run pytest -m integration -v --timeout=60 --no-cov
"""

import re
import uuid

import pytest

from tools.get_action_by_area_id import GetActionByAreaIdTool
from tools.search_actions import SearchActionsTool

pytestmark = pytest.mark.integration


def _make_get_tool(api_key: str) -> GetActionByAreaIdTool:
    return GetActionByAreaIdTool.from_credentials({"actionbook_api_key": api_key})


def _make_search_tool(api_key: str) -> SearchActionsTool:
    return SearchActionsTool.from_credentials({"actionbook_api_key": api_key})


class TestGetActionLive:
    """GetActionByAreaIdTool with real HTTP calls."""

    def test_nonexistent_area_returns_not_found(self, api_key):
        """A fabricated area_id should yield a soft 'not found' message."""
        tool = _make_get_tool(api_key)
        fake = f"fake-{uuid.uuid4().hex[:8]}.com:nonexist:element"
        results = list(tool._invoke({"area_id": fake}))

        assert len(results) == 1
        assert "not found" in results[0].message.text.lower()

    # Parameter validation – no network
    def test_empty_area_id_raises(self, api_key):
        """Empty area_id must raise ValueError."""
        tool = _make_get_tool(api_key)
        with pytest.raises(ValueError, match="required"):
            list(tool._invoke({"area_id": ""}))

    def test_invalid_format_raises(self, api_key):
        """area_id without 3 colon-separated parts must raise ValueError."""
        tool = _make_get_tool(api_key)
        with pytest.raises(ValueError, match="Invalid area_id format"):
            list(tool._invoke({"area_id": "only-one-part"}))

    def test_triple_colon_raises(self, api_key):
        """':::' has empty parts and must raise ValueError."""
        tool = _make_get_tool(api_key)
        with pytest.raises(ValueError, match="non-empty"):
            list(tool._invoke({"area_id": ":::"}))


class TestSearchGetRoundtrip:
    """The most critical E2E scenario: search -> get chain."""

    def test_search_then_get_details(self, api_key):
        """Search for 'login', extract the first area_id, then get its details.

        This mirrors the real Dify Workflow: one tool feeds another.
        """
        search_tool = _make_search_tool(api_key)
        get_tool = _make_get_tool(api_key)

        # 1. Search
        search_results = list(search_tool._invoke({"query": "login", "limit": 5}))
        assert len(search_results) == 1
        search_text = search_results[0].message.text
        assert "No results found" not in search_text, "Search returned no results for 'login'"

        # 2. Extract first Area ID from the text response
        #    Format: "Area ID: <value>"
        match = re.search(r"Area ID:\s*(.+)", search_text)
        assert match is not None, (
            f"Could not parse an Area ID from search output:\n{search_text[:300]}"
        )
        area_id = match.group(1).strip()
        assert len(area_id) > 0

        # 3. Get details
        get_results = list(get_tool._invoke({"area_id": area_id}))
        assert len(get_results) == 1
        details_text = get_results[0].message.text
        assert len(details_text.strip()) > 0
        # Should not be a "not found" message
        assert "not found" not in details_text.lower(), (
            f"get_action returned 'not found' for area_id extracted from search: {area_id}"
        )

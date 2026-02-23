"""Workflow-level unit test: search/get stale selector -> snapshot recovery -> success."""

import re
from unittest.mock import MagicMock, Mock, patch

from playwright.sync_api import TimeoutError as PlaywrightTimeout

from tools.browser_operator import BrowserOperatorTool
from tools.get_action_by_area_id import GetActionByAreaIdTool
from tools.search_actions import SearchActionsTool


def _mock_response(status_code: int, text: str) -> Mock:
    resp = Mock()
    resp.status_code = status_code
    resp.text = text
    return resp


class TestRagArxivWorkflowWithSnapshotRecovery:
    """Simulate a realistic agent workflow for the user's RAG-on-code query."""

    @patch("tools.search_actions.requests.get")
    @patch("tools.browser_operator.pool")
    def test_search_get_stale_click_then_snapshot_recovery(
        self,
        mock_pool,
        mock_search_get,
    ):
        def requests_side_effect(url, **kwargs):
            if url.endswith("/api/search_actions"):
                return _mock_response(
                    200,
                    (
                        "## Results\n\n"
                        "### arxiv.org:/search/advanced:default\n"
                        "- ID: arxiv.org:/search/advanced:default\n"
                    ),
                )
            if url.endswith("/api/get_action_by_area_id"):
                return _mock_response(
                    200,
                    (
                        "Search field selector: input[name='terms-0-term']\n"
                        "Search type selector: select[name='terms-0-field']\n"
                        "Submit selector: button.old-search-submit\n"
                    ),
                )
            raise AssertionError(f"unexpected url: {url}")

        mock_search_get.side_effect = requests_side_effect

        # 3) browser page mock:
        #    click on stale selector fails -> tool returns snapshot guidance
        page = MagicMock()
        page.url = "https://arxiv.org/search/advanced"
        page.title.return_value = "arXiv Advanced Search"

        def wait_selector_side_effect(selector, timeout):
            if selector == "button.old-search-submit":
                raise PlaywrightTimeout("stale selector timeout")
            return None

        page.wait_for_selector.side_effect = wait_selector_side_effect
        page.evaluate.return_value = {
            "tree": {
                "role": "generic",
                "children": [
                    {
                        "role": "button",
                        "children": [{"role": "text", "content": "Search"}],
                    }
                ],
            },
            "refCount": 1,
        }
        page.inner_text.return_value = "\n".join(
            [
                "1. Retrieval-Augmented Code Generation with Dynamic Memory",
                "2. RAG for Program Synthesis from API Docs",
                "3. Improving Code LLMs via Retrieval-Augmented Planning",
                "4. Multi-Stage Retrieval for Repository-Level Code Generation",
                "5. Retrieval-Augmented Tool Use for Code Completion",
                "6. Sparse-Dense Hybrid Retrieval for Code Generation",
                "7. Long-Context RAG for Software Engineering Tasks",
                "8. RAG-Enhanced Bug-Fix Generation with Test Feedback",
                "9. Retrieval-Augmented Agents for Code Authoring",
                "10. Citation-Aware RAG for Code Generation Research",
            ]
        )
        mock_pool.get_page.return_value = page

        search_tool = SearchActionsTool.from_credentials({"actionbook_api_key": ""})
        get_tool = GetActionByAreaIdTool.from_credentials({"actionbook_api_key": ""})
        op_tool = BrowserOperatorTool.from_credentials({})

        # Step A: search
        search_result = list(
            search_tool._invoke(
                {
                    "domain": "arxiv.org",
                    "query": (
                        "advanced search retrieval augmented code cs.CL cs.SE "
                        "2025-12-01 relevance citations"
                    ),
                }
            )
        )[0].message.text
        assert "arxiv.org:/search/advanced:default" in search_result

        area_id_match = re.search(r"arxiv\.org:/search/advanced:default", search_result)
        assert area_id_match is not None
        area_id = area_id_match.group(0)

        # Step B: get action details (contains stale submit selector)
        get_result = list(get_tool._invoke({"area_id": area_id}))[0].message.text
        assert "button.old-search-submit" in get_result

        # Step C: navigate + fill query + click (stale selector triggers snapshot guidance)
        nav = list(
            op_tool._invoke(
                {
                    "session_id": "sess-rag-arxiv",
                    "action": "navigate",
                    "url": "https://arxiv.org/search/advanced",
                }
            )
        )[0].message.text
        assert "Navigation successful" in nav

        list(
            op_tool._invoke(
                {
                    "session_id": "sess-rag-arxiv",
                    "action": "fill",
                    "selector": "input[name='terms-0-term']",
                    "text": "retrieval augmented code",
                }
            )
        )

        click_msg = list(
            op_tool._invoke(
                {
                    "session_id": "sess-rag-arxiv",
                    "action": "click",
                    "selector": "button.old-search-submit",
                    "timeout_ms": 500,
                }
            )
        )[0].message.text
        assert "action=snapshot" in click_msg
        assert "[ref=eN]" in click_msg

        # Step D: read top-10 titles text
        text_msg = list(
            op_tool._invoke(
                {
                    "session_id": "sess-rag-arxiv",
                    "action": "get_text",
                }
            )
        )[0].message.text
        assert "1. Retrieval-Augmented Code Generation with Dynamic Memory" in text_msg
        assert "10. Citation-Aware RAG for Code Generation Research" in text_msg

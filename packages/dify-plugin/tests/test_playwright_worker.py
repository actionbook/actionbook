"""Unit tests for JS selector behavior in playwright_worker."""

import sys
from pathlib import Path
from unittest.mock import MagicMock

sys.path.insert(0, str(Path(__file__).parent.parent))

from utils.playwright_worker import _is_js_selector, _js_wait_for_selector


class TestJsSelectorDetection:
    def test_detects_ref_selector(self):
        assert _is_js_selector("[ref=e5]")
        assert _is_js_selector("@e5")

    def test_detects_xpath_selector(self):
        assert _is_js_selector("//button[text()='Submit']")
        assert _is_js_selector("(//button)[1]")

    def test_css_selector_not_treated_as_js_selector(self):
        assert not _is_js_selector("button.submit")


class TestJsWaitForSelector:
    def test_wait_for_selector_uses_wait_for_function_with_timeout(self):
        page = MagicMock()

        _js_wait_for_selector(page, "[ref=e1]", timeout=1500.0)

        page.wait_for_function.assert_called_once()
        _, kwargs = page.wait_for_function.call_args
        assert kwargs["timeout"] == 1500.0


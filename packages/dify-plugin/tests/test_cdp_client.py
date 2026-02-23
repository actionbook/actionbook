"""Tests for utils/cdp_client.py — validate_cdp_url."""

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent))

from utils.cdp_client import validate_cdp_url


class TestValidateCdpUrl:
    @pytest.mark.parametrize("url", [
        "ws://localhost:9222",
        "wss://production-sfo.browserless.io?token=TOKEN",
        "http://localhost:9222",
        "https://cloud-browser.example.com",
    ])
    def test_valid_urls_returned_stripped(self, url):
        assert validate_cdp_url(f"  {url}  ") == url

    @pytest.mark.parametrize("bad", ["", "   ", None])
    def test_empty_raises(self, bad):
        with pytest.raises(ValueError, match="required"):
            validate_cdp_url(bad)

    @pytest.mark.parametrize("bad", [
        "ftp://host",
        "localhost:9222",
        "//host:9222",
        "random-string",
    ])
    def test_invalid_prefix_raises(self, bad):
        with pytest.raises(ValueError, match="Invalid cdp_url"):
            validate_cdp_url(bad)

    def test_insecure_non_localhost_warns(self, caplog):
        """ws:// to a remote host should log a warning."""
        import logging
        with caplog.at_level(logging.WARNING, logger="utils.cdp_client"):
            result = validate_cdp_url("ws://remote-host:9222")
        assert result == "ws://remote-host:9222"
        assert "Insecure CDP connection" in caplog.text

    @pytest.mark.parametrize("url", [
        "ws://localhost:9222",
        "ws://127.0.0.1:9222",
        "http://localhost:9222",
    ])
    def test_insecure_localhost_no_warning(self, url, caplog):
        """ws:// to localhost should not log a warning."""
        import logging
        with caplog.at_level(logging.WARNING, logger="utils.cdp_client"):
            validate_cdp_url(url)
        assert "Insecure CDP connection" not in caplog.text

    def test_secure_remote_no_warning(self, caplog):
        """wss:// to remote host should not log a warning."""
        import logging
        with caplog.at_level(logging.WARNING, logger="utils.cdp_client"):
            validate_cdp_url("wss://remote-host:9222")
        assert "Insecure CDP connection" not in caplog.text

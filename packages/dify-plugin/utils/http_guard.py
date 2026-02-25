"""HTTP response guards for common misconfiguration patterns."""

from __future__ import annotations

from typing import Any


def looks_like_html_response(response: Any, body_text: str) -> bool:
    """Return True when an API call appears to have returned an HTML page."""
    content_type = str((getattr(response, "headers", {}) or {}).get("Content-Type", "")).lower()
    sniff = str(body_text or "").lstrip().lower()
    return (
        "text/html" in content_type
        or sniff.startswith("<!doctype html")
        or sniff.startswith("<html")
    )


def build_html_misroute_message(api_base_url: str, request_url: str) -> str:
    """Build a concise troubleshooting message for HTML misroute responses."""
    return (
        "Error: Received an HTML page instead of the expected API response.\n"
        f"Request URL: {request_url}\n"
        f"Current ACTIONBOOK_API_URL: {api_base_url}\n\n"
        "Likely cause: ACTIONBOOK_API_URL points to a non-Actionbook endpoint "
        "(commonly Dify app API such as https://api.dify.ai/v1).\n"
        "Fix: set ACTIONBOOK_API_URL to https://api.actionbook.dev."
    )

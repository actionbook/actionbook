"""Shared constants for Actionbook Dify Plugin."""

import os

# Actionbook API base URL (configurable via environment variable)
API_BASE_URL = os.environ.get("ACTIONBOOK_API_URL", "https://api.actionbook.dev")

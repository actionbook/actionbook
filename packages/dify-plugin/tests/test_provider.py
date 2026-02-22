"""Tests for ActionbookProvider credential validation."""

from _plugin import ActionbookProvider


class TestActionbookProvider:
    """Test ActionbookProvider credential validation."""

    def test_valid_credentials(self):
        """Test credential validation passes (no credentials required)."""
        provider = ActionbookProvider()
        credentials = {"actionbook_api_key": "valid_key_123"}

        # Should not raise exception - public API, no validation needed
        provider._validate_credentials(credentials)

    def test_missing_api_key_passes_validation(self):
        """Test that missing API key is accepted (public access)."""
        provider = ActionbookProvider()
        credentials = {}

        # Should not raise - no credentials required
        provider._validate_credentials(credentials)

    def test_empty_api_key_passes_validation(self):
        """Test that empty string API key is accepted."""
        provider = ActionbookProvider()
        credentials = {"actionbook_api_key": "  "}

        # Should not raise - no credentials required
        provider._validate_credentials(credentials)

    def test_empty_credentials_dict(self):
        """Test that empty credentials dict is accepted."""
        provider = ActionbookProvider()

        provider._validate_credentials({})

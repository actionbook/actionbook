"""Browser provider registry.

To add a new provider:
1. Create providers/{name}.py implementing BrowserSession + BrowserProvider
2. Import the class in _REGISTRY below
3. Add the key to _REGISTRY

Supported providers:
- hyperbrowser: Profile-based persistence, bills only active time
- steel:        Session-ID reconnect, self-hostable (not yet implemented)
"""

from providers.base import BrowserProvider, BrowserSession
from providers.hyperbrowser import HyperbrowserProvider
from providers.steel import SteelProvider

_REGISTRY: dict[str, type[BrowserProvider]] = {
    "hyperbrowser": HyperbrowserProvider,
    "steel": SteelProvider,
}

SUPPORTED_PROVIDERS = list(_REGISTRY.keys())


def get_provider(name: str, api_key: str) -> BrowserProvider:
    """
    Instantiate a browser provider by name.

    Args:
        name:    Provider key (e.g., "hyperbrowser", "steel").
        api_key: API key for the provider.

    Returns:
        A BrowserProvider instance.

    Raises:
        ValueError: Unknown provider name.
        NotImplementedError: Provider registered but not yet implemented.
    """
    cls = _REGISTRY.get(name)
    if cls is None:
        supported = ", ".join(SUPPORTED_PROVIDERS)
        raise ValueError(
            f"Unknown provider: '{name}'. "
            f"Supported providers: {supported}"
        )
    return cls(api_key=api_key)


__all__ = [
    "BrowserProvider",
    "BrowserSession",
    "SUPPORTED_PROVIDERS",
    "get_provider",
]

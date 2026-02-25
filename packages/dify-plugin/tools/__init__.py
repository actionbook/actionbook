"""Actionbook Dify Plugin Tools.

Browser tool classes are imported lazily to avoid loading playwright and
connection_pool at module level.  The non-browser tools (search_actions,
get_action_by_area_id) are imported eagerly so they remain fast.
"""

from .get_action_by_area_id import GetActionByAreaIdTool
from .search_actions import SearchActionsTool


def __getattr__(name: str):
    """Lazy-load browser tool classes on first access and cache in module globals."""
    _lazy_map = {
        "BrowserCreateSessionTool": ".browser_create_session",
        "BrowserOperatorTool": ".browser_operator",
        "BrowserStopSessionTool": ".browser_stop_session",
    }
    if name in _lazy_map:
        import importlib
        mod = importlib.import_module(_lazy_map[name], __name__)
        cls = getattr(mod, name)
        globals()[name] = cls  # cache so __getattr__ is not called again
        return cls
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


__all__ = [
    "BrowserCreateSessionTool",
    "BrowserOperatorTool",
    "BrowserStopSessionTool",
    "GetActionByAreaIdTool",
    "SearchActionsTool",
]

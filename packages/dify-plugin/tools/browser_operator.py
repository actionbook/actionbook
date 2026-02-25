"""Browser Operator Tool — unified browser page operation dispatcher.

Consolidates 15 individual browser tools into one with action-based dispatch.
All action-specific parameter validation is performed by ``_pre_validate``
*before* the CDP connection is established, so handler functions assume their
required parameters are already present.
"""

import ipaddress
import logging
import socket
import uuid
from collections.abc import Callable, Generator
from typing import Any
from urllib.parse import urlparse

from dify_plugin import Tool
from dify_plugin.entities.tool import ToolInvokeMessage

from utils.cdp_client import validate_cdp_url

# Lazy imports — avoid loading playwright/connection_pool at module level.
# These modules require the Playwright Node.js driver binary and spawn
# background threads/subprocesses, which may not be available in Dify Cloud's
# serverless runtime.
#
# Fallback exception stubs are defined eagerly so that except clauses always
# reference valid BaseException subclasses, even before _ensure_browser_deps()
# runs.  After a successful import the stubs are replaced by the real types.
import threading

_pool = None
_lazy_imported = False
_lazy_lock = threading.Lock()


class _StubException(Exception):
    """Placeholder exception type used before lazy imports complete."""


PlaywrightTimeout: type[Exception] = _StubException
ConnectionNotFound: type[Exception] = _StubException
ConnectionUnhealthy: type[Exception] = _StubException
WorkerTimeoutError: type[Exception] = _StubException


pool = None  # Module-level alias set by _ensure_browser_deps(); patchable by tests.


def _ensure_browser_deps():
    """Lazy-import browser dependencies on first use (thread-safe)."""
    global pool, _pool, _lazy_imported, PlaywrightTimeout, ConnectionNotFound, ConnectionUnhealthy, WorkerTimeoutError
    if _lazy_imported:
        return
    with _lazy_lock:
        if _lazy_imported:
            return
        try:
            from playwright.sync_api import TimeoutError as _PT
            PlaywrightTimeout = _PT
        except Exception:
            pass  # keep _StubException fallback

        from utils.connection_pool import (
            ConnectionNotFound as _CNF,
            ConnectionUnhealthy as _CU,
            WorkerTimeoutError as _WTE,
            pool as _p,
        )
        ConnectionNotFound = _CNF
        ConnectionUnhealthy = _CU
        WorkerTimeoutError = _WTE
        _pool = _p
        pool = _p  # update module-level alias
        # Only mark as done AFTER all imports succeed.
        _lazy_imported = True

logger = logging.getLogger(__name__)


def _safe_timeout(raw: Any, default: int = 30000) -> int:
    """Safely convert a raw timeout value to int, returning *default* on failure."""
    try:
        return max(0, int(raw))
    except (TypeError, ValueError):
        return default


# ---------------------------------------------------------------------------
# Action handlers
# Pre-conditions: _pre_validate has already run, so required params are present.
# Signature: (tool, page, params) -> Generator[ToolInvokeMessage, None, None]
# ---------------------------------------------------------------------------


def _handle_navigate(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Navigate to a URL."""
    url = (params.get("url") or "").strip()
    timeout_ms = _safe_timeout(params.get("timeout_ms"))

    page.goto(url, timeout=float(timeout_ms), wait_until="domcontentloaded")
    yield tool.create_text_message(
        f"Navigation successful.\nURL: {page.url}\nTitle: {page.title()}"
    )


def _handle_click(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Wait for element then click it. Reports failure directly for LLM to decide next step."""
    selector = (params.get("selector") or "").strip()
    # Click uses a shorter default (10s) — waiting 30s for a missing element wastes time.
    timeout_ms = _safe_timeout(params.get("timeout_ms"), default=10000)

    try:
        if timeout_ms > 0:
            page.wait_for_selector(selector, timeout=float(timeout_ms))
        page.click(selector)
        yield tool.create_text_message(f"Clicked: '{selector}'")
    except (ConnectionNotFound, ConnectionUnhealthy):
        raise
    except (PlaywrightTimeout, WorkerTimeoutError):
        logger.debug("wait_for_selector timed out for '%s'", selector, exc_info=True)
        yield tool.create_text_message(
            f"Element not found: '{selector}' within {timeout_ms}ms. "
            "Use action=snapshot to see available elements, then use [ref=eN] as selector "
            "(e.g. selector=\"[ref=e5]\"). This is the most reliable approach. "
            "For form submission, use action=press_key key=Enter. "
            "IMPORTANT: If giving up, call browser_stop_session."
        )
    except Exception as e:
        logger.debug("click failed for '%s'", selector, exc_info=True)
        yield tool.create_text_message(
            f"Click failed for '{selector}': {type(e).__name__}: {e}. "
            "Use action=snapshot then use [ref=eN] as selector for reliable element targeting. "
            "For form submission, use action=press_key key=Enter. "
            "IMPORTANT: If giving up, call browser_stop_session."
        )


def _handle_type(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Type text character-by-character into an input (appends)."""
    selector = (params.get("selector") or "").strip()
    text = params.get("text") or ""

    page.type(selector, text)
    yield tool.create_text_message(f"Typed {len(text)} characters into '{selector}'.")


def _handle_fill(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Clear an input then fill it with text (atomic set). Empty string clears the field."""
    selector = (params.get("selector") or "").strip()
    text = params.get("text") or ""

    page.fill(selector, text)
    yield tool.create_text_message(f"Filled '{selector}' ({len(text)} chars).")


def _handle_select(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Select an option by value in a <select> element."""
    selector = (params.get("selector") or "").strip()
    value = (params.get("value") or "").strip()

    selected = page.select_option(selector, value=value)
    if not selected:
        yield tool.create_text_message(
            f"No option with value='{value}' found in '{selector}'."
        )
        return
    yield tool.create_text_message(f"Selected '{value}' in '{selector}'.")


def _handle_press_key(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Send a keyboard key press."""
    key = (params.get("key") or "").strip()

    page.keyboard.press(key)
    yield tool.create_text_message(f"Pressed key: '{key}'")


def _handle_hover(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Move mouse over an element to trigger hover effects."""
    selector = (params.get("selector") or "").strip()

    page.hover(selector)
    yield tool.create_text_message(f"Hovered over: '{selector}'")


def _handle_get_text(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Extract visible text from the page or a specific element."""
    selector = (params.get("selector") or "").strip() or None

    try:
        text = page.inner_text(selector or "body")
    except Exception as e:
        msg = (
            f"No element found for selector: '{selector}'"
            if selector
            else f"Failed to read page text: {type(e).__name__}: {e}"
        )
        yield tool.create_text_message(msg)
        return

    yield tool.create_text_message(text or "(empty)")


def _handle_get_html(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Retrieve HTML of the page or a specific element."""
    selector = (params.get("selector") or "").strip() or None

    try:
        html = page.inner_html(selector) if selector else page.content()
    except Exception as e:
        msg = (
            f"No element found for selector: '{selector}'"
            if selector
            else f"Failed to read page HTML: {type(e).__name__}: {e}"
        )
        yield tool.create_text_message(msg)
        return

    yield tool.create_text_message(html or "(empty)")


def _handle_wait(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Wait for an element to appear in the DOM."""
    selector = (params.get("selector") or "").strip()
    timeout_ms = _safe_timeout(params.get("timeout_ms"))

    try:
        page.wait_for_selector(selector, timeout=float(timeout_ms))
        yield tool.create_text_message(f"Element found: '{selector}'")
    except (PlaywrightTimeout, WorkerTimeoutError):
        logger.debug("wait_for_selector timed out for '%s'", selector, exc_info=True)
        yield tool.create_text_message(
            f"Element not found: '{selector}' within {timeout_ms}ms."
        )


def _handle_wait_navigation(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Wait for page navigation/load to complete."""
    timeout_ms = _safe_timeout(params.get("timeout_ms"))

    try:
        page.wait_for_load_state("domcontentloaded", timeout=float(timeout_ms))
        yield tool.create_text_message(
            f"Navigation complete.\nURL: {page.url}\nTitle: {page.title()}"
        )
    except Exception:
        logger.debug("wait_for_load_state timed out", exc_info=True)
        yield tool.create_text_message(
            f"Navigation did not complete within {timeout_ms}ms. "
            f"Current URL: {page.url}"
        )


def _handle_go_back(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Navigate to the previous page in history."""
    page.go_back()
    yield tool.create_text_message(f"Navigated back.\nURL: {page.url}")


def _handle_go_forward(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Navigate to the next page in history."""
    page.go_forward()
    yield tool.create_text_message(f"Navigated forward.\nURL: {page.url}")


def _handle_reload(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Reload the current page."""
    page.reload(wait_until="domcontentloaded")
    yield tool.create_text_message(f"Page reloaded.\nURL: {page.url}")


# Accessibility-tree snapshot JS — ported from actionbook-rs browser.rs
_SNAPSHOT_JS = r"""
(function() {
    const SKIP_TAGS = new Set([
        'script', 'style', 'noscript', 'template', 'svg',
        'path', 'defs', 'clippath', 'lineargradient', 'stop',
        'meta', 'link', 'br', 'wbr'
    ]);
    const INLINE_TAGS = new Set([
        'strong', 'b', 'em', 'i', 'code', 'span', 'small',
        'sup', 'sub', 'abbr', 'mark', 'u', 's', 'del', 'ins',
        'time', 'q', 'cite', 'dfn', 'var', 'samp', 'kbd'
    ]);
    const INTERACTIVE_ROLES = new Set([
        'button', 'link', 'textbox', 'checkbox', 'radio', 'combobox',
        'listbox', 'menuitem', 'menuitemcheckbox', 'menuitemradio',
        'option', 'searchbox', 'slider', 'spinbutton', 'switch',
        'tab', 'treeitem'
    ]);
    const CONTENT_ROLES = new Set([
        'heading', 'cell', 'gridcell', 'columnheader', 'rowheader',
        'listitem', 'article', 'region', 'main', 'navigation', 'img'
    ]);

    function getRole(el) {
        const explicit = el.getAttribute('role');
        if (explicit) return explicit.toLowerCase();
        const tag = el.tagName.toLowerCase();
        if (INLINE_TAGS.has(tag)) return tag;
        const roleMap = {
            'a': el.hasAttribute('href') ? 'link' : 'generic',
            'button': 'button',
            'input': getInputRole(el),
            'select': 'combobox',
            'textarea': 'textbox',
            'img': 'img',
            'h1': 'heading', 'h2': 'heading', 'h3': 'heading',
            'h4': 'heading', 'h5': 'heading', 'h6': 'heading',
            'nav': 'navigation', 'main': 'main',
            'header': 'banner', 'footer': 'contentinfo',
            'aside': 'complementary', 'form': 'form',
            'table': 'table',
            'thead': 'rowgroup', 'tbody': 'rowgroup', 'tfoot': 'rowgroup',
            'tr': 'row', 'th': 'columnheader', 'td': 'cell',
            'ul': 'list', 'ol': 'list', 'li': 'listitem',
            'details': 'group', 'summary': 'button',
            'dialog': 'dialog',
            'section': el.hasAttribute('aria-label') || el.hasAttribute('aria-labelledby') ? 'region' : 'generic',
            'article': 'article'
        };
        return roleMap[tag] || 'generic';
    }

    function getInputRole(el) {
        const type = (el.getAttribute('type') || 'text').toLowerCase();
        const map = {
            'text': 'textbox', 'email': 'textbox', 'password': 'textbox',
            'search': 'searchbox', 'tel': 'textbox', 'url': 'textbox',
            'number': 'spinbutton',
            'checkbox': 'checkbox', 'radio': 'radio',
            'submit': 'button', 'reset': 'button', 'button': 'button',
            'range': 'slider'
        };
        return map[type] || 'textbox';
    }

    function getAccessibleName(el) {
        const ariaLabel = el.getAttribute('aria-label');
        if (ariaLabel) return ariaLabel.trim();
        const labelledBy = el.getAttribute('aria-labelledby');
        if (labelledBy) {
            const label = document.getElementById(labelledBy);
            if (label) return (label.textContent || '').trim().substring(0, 100);
        }
        const tag = el.tagName.toLowerCase();
        if (tag === 'img') return el.getAttribute('alt') || '';
        if (tag === 'input' || tag === 'textarea' || tag === 'select') {
            if (el.id) {
                const label = document.querySelector('label[for="' + el.id + '"]');
                if (label) return (label.textContent || '').trim().substring(0, 100);
            }
            return el.getAttribute('placeholder') || el.getAttribute('title') || '';
        }
        if (tag === 'a' || tag === 'button' || tag === 'summary') return '';
        if (['h1','h2','h3','h4','h5','h6'].includes(tag)) {
            return (el.textContent || '').trim().substring(0, 150);
        }
        const title = el.getAttribute('title');
        if (title) return title.trim();
        return '';
    }

    function isHidden(el) {
        if (el.hidden) return true;
        if (el.getAttribute('aria-hidden') === 'true') return true;
        const style = el.style;
        if (style.display === 'none' || style.visibility === 'hidden') return true;
        if (el.offsetParent === null && el.tagName.toLowerCase() !== 'body' &&
            getComputedStyle(el).position !== 'fixed' && getComputedStyle(el).position !== 'sticky') {
            const cs = getComputedStyle(el);
            if (cs.display === 'none' || cs.visibility === 'hidden') return true;
        }
        return false;
    }

    let refCounter = 0;

    function walk(el, depth) {
        if (depth > 15) return null;
        const tag = el.tagName.toLowerCase();
        if (SKIP_TAGS.has(tag)) return null;
        if (isHidden(el)) return null;

        const role = getRole(el);
        const name = getAccessibleName(el);
        const isInteractive = INTERACTIVE_ROLES.has(role);
        const isContent = CONTENT_ROLES.has(role);
        const shouldRef = isInteractive || (isContent && name);

        let ref = null;
        if (shouldRef) { refCounter++; ref = 'e' + refCounter; }

        const children = [];
        for (const child of el.childNodes) {
            if (child.nodeType === 1) {
                const c = walk(child, depth + 1);
                if (c) children.push(c);
            } else if (child.nodeType === 3) {
                const t = (child.textContent || '').trim();
                if (t) {
                    const content = t.length > 200 ? t.substring(0, 200) + '...' : t;
                    children.push({ role: 'text', content });
                }
            }
        }

        if (role === 'generic' && !name && !ref && children.length === 1) return children[0];
        if (role === 'generic' && !name && !ref && children.length === 0) return null;

        const node = { role };
        if (name) node.name = name;
        if (ref) node.ref = ref;
        if (children.length > 0) node.children = children;
        if (role === 'link') { const href = el.getAttribute('href'); if (href) node.url = href; }
        if (role === 'heading') { const m = tag.match(/^h(\d)$/); if (m) node.level = parseInt(m[1]); }
        if (role === 'textbox' || role === 'searchbox') node.value = el.value || '';
        if (role === 'checkbox' || role === 'radio' || role === 'switch') node.checked = el.checked || false;
        return node;
    }

    const tree = walk(document.body, 0);
    return { tree, refCount: refCounter };
})()
"""


def _render_snapshot_node(node: dict[str, Any], depth: int = 0) -> str:
    """Render a snapshot tree node as indented text lines.

    Output format matches actionbook-rs render_snapshot_tree:
      - heading "Title" [ref=e1] [level=1]
      - button "Submit" [ref=e2]
      - link "Home" [ref=e3]:
        - /url: https://example.com
        - text: Home
    """
    indent = "  " * depth
    role = node.get("role", "generic")

    # Text nodes
    if role == "text":
        content = node.get("content", "")
        return f"{indent}- text: {content}\n" if content else ""

    name = node.get("name")
    ref_id = node.get("ref")
    url = node.get("url")
    children = node.get("children", [])
    has_children = bool(children)

    # Build line: - role "name" [ref=eN] [extra]
    line = f"{indent}- {role}"
    if name:
        line += f' "{name}"'
    if ref_id:
        line += f" [ref={ref_id}]"

    level = node.get("level")
    if level is not None:
        line += f" [level={level}]"
    checked = node.get("checked")
    if checked is not None:
        line += f" [checked={'true' if checked else 'false'}]"
    value = node.get("value")
    if value:
        line += f' [value="{value}"]'

    if has_children or url:
        line += ":"
    line += "\n"

    # URL for links
    if url:
        line += f"{indent}  - /url: {url}\n"

    # Children
    for child in children:
        line += _render_snapshot_node(child, depth + 1)

    return line


def _handle_snapshot(
    tool: Tool, page: Any, params: dict[str, Any],
) -> Generator[ToolInvokeMessage, None, None]:
    """Capture an accessibility-tree snapshot of the current page.

    Returns a structured text representation of all interactive and content
    elements with ref identifiers that can be used as selectors.
    """
    result = page.evaluate(_SNAPSHOT_JS)
    tree = result.get("tree") if isinstance(result, dict) else None
    ref_count = result.get("refCount", 0) if isinstance(result, dict) else 0

    if tree is None:
        yield tool.create_text_message("Snapshot: page is empty or could not be parsed.")
        return

    output = _render_snapshot_node(tree)
    header = f"Page snapshot — URL: {page.url}\nInteractive elements: {ref_count}\n\n"
    yield tool.create_text_message(header + output)


# ---------------------------------------------------------------------------
# Dispatch table
# ---------------------------------------------------------------------------

_HandlerFn = Callable[
    [Tool, Any, dict[str, Any]],
    Generator[ToolInvokeMessage, None, None],
]

_HANDLERS: dict[str, _HandlerFn] = {
    "navigate": _handle_navigate,
    "click": _handle_click,
    "type": _handle_type,
    "fill": _handle_fill,
    "select": _handle_select,
    "press_key": _handle_press_key,
    "hover": _handle_hover,
    "get_text": _handle_get_text,
    "get_html": _handle_get_html,
    "wait": _handle_wait,
    "wait_navigation": _handle_wait_navigation,
    "go_back": _handle_go_back,
    "go_forward": _handle_go_forward,
    "reload": _handle_reload,
    "snapshot": _handle_snapshot,
}


_BLOCKED_IP_NETWORKS = [
    ipaddress.ip_network("127.0.0.0/8"),
    ipaddress.ip_network("10.0.0.0/8"),
    ipaddress.ip_network("172.16.0.0/12"),
    ipaddress.ip_network("192.168.0.0/16"),
    ipaddress.ip_network("169.254.0.0/16"),
    ipaddress.ip_network("100.64.0.0/10"),
    ipaddress.ip_network("::1/128"),
    ipaddress.ip_network("fc00::/7"),
]

_BLOCKED_HOSTNAMES = frozenset({"localhost", "metadata.google.internal"})


def _is_blocked_ip(ip: ipaddress.IPv4Address | ipaddress.IPv6Address) -> bool:
    """Return True if an IP belongs to blocked private/internal networks."""
    return any(ip in net for net in _BLOCKED_IP_NETWORKS)


def _parse_ip_host(host: str) -> ipaddress.IPv4Address | ipaddress.IPv6Address | None:
    """Parse a host string as IPv4/IPv6, including legacy IPv4 forms."""
    try:
        return ipaddress.ip_address(host)
    except ValueError:
        # socket.inet_aton accepts legacy IPv4 notations:
        # 127.1, 2130706433, 0x7f000001, 017700000001
        try:
            return ipaddress.ip_address(socket.inet_aton(host))
        except OSError:
            return None


def _resolve_host_ips(host: str) -> set[ipaddress.IPv4Address | ipaddress.IPv6Address]:
    """Resolve DNS A/AAAA records and return parsed IP addresses."""
    resolved: set[ipaddress.IPv4Address | ipaddress.IPv6Address] = set()
    try:
        for info in socket.getaddrinfo(host, None):
            family = info[0]
            sockaddr = info[4]
            if family not in (socket.AF_INET, socket.AF_INET6):
                continue
            ip = _parse_ip_host(sockaddr[0])
            if ip is not None:
                resolved.add(ip)
    except socket.gaierror:
        return set()
    return resolved


def _is_ssrf_target(url: str) -> bool:
    """Check if a URL targets a private/internal network address.

    Security note — TOCTOU / DNS rebinding limitation:
    This function resolves DNS at validation time, but the actual navigation
    happens later when the browser engine re-resolves the hostname. An attacker
    could exploit DNS rebinding: first query returns a safe IP, second query
    returns a private IP (e.g. 127.0.0.1).

    Mitigation: In the Dify plugin context, navigation occurs in a remote cloud
    browser (Hyperbrowser/Steel), not on the plugin host. DNS rebinding would
    affect the cloud browser's network, not the plugin server. The cloud browser
    provider's own network policies provide an additional layer of protection.

    A full fix (resolve once → navigate to IP with Host header) is not feasible
    with Playwright's navigation API. This check blocks the common case of
    direct private IP/hostname usage.
    """
    try:
        parsed = urlparse(url)
        host = (parsed.hostname or "").strip().rstrip(".").lower()
        if not host:
            return False

        if host in _BLOCKED_HOSTNAMES or host.endswith(".localhost"):
            return True

        direct_ip = _parse_ip_host(host)
        if direct_ip is not None:
            return _is_blocked_ip(direct_ip)

        # Domain host: resolve and block if any answer points to internal ranges.
        resolved_ips = _resolve_host_ips(host)
        return any(_is_blocked_ip(ip) for ip in resolved_ips)
    except ValueError:
        return False


def _pre_validate(action: str, params: dict[str, Any]) -> str | None:
    """Validate action-specific required params before establishing CDP connection.

    Returns an error message string if validation fails, None if valid.
    """
    url = (params.get("url") or "").strip()
    selector = (params.get("selector") or "").strip()
    text = params.get("text") or ""
    value = (params.get("value") or "").strip()
    key = (params.get("key") or "").strip()

    if action == "navigate":
        if not url:
            return "Error: 'url' is required for action 'navigate'."
        if not url.startswith(("http://", "https://")):
            return f"Error: 'url' must start with http:// or https://. Got: '{url}'"
        if _is_ssrf_target(url):
            return f"Error: Navigation to private/internal addresses is not permitted: '{url}'"
    elif action in ("click", "hover"):
        if not selector:
            return f"Error: 'selector' is required for action '{action}'."
    elif action == "type":
        if not selector:
            return "Error: 'selector' is required for action 'type'."
        if text == "":
            return "Error: 'text' is required for action 'type'."
    elif action == "fill":
        if not selector:
            return "Error: 'selector' is required for action 'fill'."
    elif action == "select":
        if not selector:
            return "Error: 'selector' is required for action 'select'."
        if not value:
            return "Error: 'value' is required for action 'select'."
    elif action == "press_key":
        if not key:
            return "Error: 'key' is required for action 'press_key'."
    elif action == "wait":
        if not selector:
            return "Error: 'selector' is required for action 'wait'."
    return None


class BrowserOperatorTool(Tool):
    def _invoke(self, tool_parameters: dict[str, Any]) -> Generator[ToolInvokeMessage, None, None]:
        # Lazy-load browser dependencies (playwright, connection_pool).
        # This allows search_actions/get_action tools to work even when
        # playwright is unavailable (e.g. Dify Cloud serverless runtime).
        # Lazy-load browser dependencies. Skip if pool is already set
        # (e.g. tests patching tools.browser_operator.pool directly).
        if pool is None:
            try:
                _ensure_browser_deps()
            except Exception as e:
                yield self.create_text_message(
                    f"Error: Browser dependencies unavailable: {type(e).__name__}: {e}\n"
                    "Playwright and subprocess support are required for browser operations. "
                    "This environment may not support browser tools."
                )
                return

        session_id = (tool_parameters.get("session_id") or "").strip()
        raw_cdp_url = (tool_parameters.get("cdp_url") or "").strip()
        action = (tool_parameters.get("action") or "").strip()

        # Validate cdp_url scheme early (before any pool/network work).
        cdp_url = ""
        if raw_cdp_url:
            try:
                cdp_url = validate_cdp_url(raw_cdp_url)
            except ValueError as e:
                yield self.create_text_message(f"Error: {e}")
                return

        if not session_id and not cdp_url:
            yield self.create_text_message(
                "Error: Either 'session_id' or 'cdp_url' is required."
            )
            return
        if not action:
            yield self.create_text_message("Error: 'action' is required.")
            return
        if action not in _HANDLERS:
            valid = ", ".join(sorted(_HANDLERS))
            yield self.create_text_message(
                f"Error: Unknown action '{action}'. Valid actions: {valid}"
            )
            return

        # Validate action-specific params before establishing CDP connection
        pre_error = _pre_validate(action, tool_parameters)
        if pre_error:
            yield self.create_text_message(pre_error)
            return

        handler = _HANDLERS[action]

        # Strategy:
        # 1) Prefer existing pooled session_id connection
        # 2) If missing/unhealthy and cdp_url exists, reconnect the pool for this session_id
        # 3) If only cdp_url exists, create an ephemeral pooled connection for this call
        if session_id:
            # Snapshot provider metadata *before* get_page(), because
            # ConnectionUnhealthy evicts the connection (and its metadata)
            # from the pool before raising.  Without this, a reconnect via
            # pool.connect() would use default empty values, causing
            # browser_stop_session to fail later (remote session leak).
            _session_meta = pool.get_session_info(session_id)
            _recover_provider = _session_meta[0] if _session_meta else "hyperbrowser"
            _recover_api_key = _session_meta[1] if _session_meta else ""

            try:
                page = pool.get_page(session_id)
                yield from handler(self, page, tool_parameters)
                return
            except (ConnectionNotFound, ConnectionUnhealthy) as e:
                if not cdp_url:
                    yield self.create_text_message(
                        f"Error: {e}\n"
                        "Recovery hint: pass `cdp_url` using the `ws_endpoint` "
                        "returned by browser_create_session.\n"
                        "IMPORTANT: If you are done or giving up, call browser_stop_session to release the session."
                    )
                    return
                logger.warning(
                    "Pool lookup failed; attempting reconnect from cdp_url for session recovery."
                )
                try:
                    pool.connect(
                        session_id, cdp_url,
                        provider_name=_recover_provider,
                        api_key=_recover_api_key,
                    )
                    page = pool.get_page(session_id)
                    yield from handler(self, page, tool_parameters)
                    return
                except Exception as reconnect_error:
                    yield self.create_text_message(
                        "Error: Failed to recover browser session from `cdp_url`. "
                        f"{type(reconnect_error).__name__}: {reconnect_error}\n"
                        "IMPORTANT: Call browser_stop_session to release the session."
                    )
                    return
            except Exception as e:
                logger.error("browser_operator session action failed.")
                yield self.create_text_message(
                    f"Error: Action '{action}' failed: {type(e).__name__}: {e}\n"
                    "IMPORTANT: Call browser_stop_session to release the session."
                )
                return

        # cdp_url-only mode: one operation in an ephemeral pooled session.
        # This avoids direct sync Playwright calls in the current runtime thread.
        ephemeral_session_id = f"ephemeral-{uuid.uuid4()}"
        try:
            pool.connect(ephemeral_session_id, cdp_url)
            page = pool.get_page(ephemeral_session_id)
            yield from handler(self, page, tool_parameters)
        except Exception as e:
            logger.error("browser_operator action failed.")
            yield self.create_text_message(
                f"Error: Action '{action}' failed: {type(e).__name__}: {e}"
            )
        finally:
            pool.disconnect(ephemeral_session_id)

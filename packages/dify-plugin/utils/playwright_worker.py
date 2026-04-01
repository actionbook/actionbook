"""Standalone Playwright worker process.

Runs outside Dify's patched runtime and executes browser operations over JSONL.

Supports three selector formats (matching actionbook CLI behavior):
  - CSS selectors: ".btn", "#submit", "button:has-text('OK')"
  - XPath selectors: "//button[text()='Submit']"
  - Snapshot refs: "@e5" or "[ref=e5]" (resolved via DOM re-walk)
"""

from __future__ import annotations

import json
import re
import sys
from typing import Any

from playwright.sync_api import sync_playwright

# Matches @eN or [ref=eN] snapshot reference selectors.
_REF_RE = re.compile(r"^(?:@e(\d+)|\[ref=e(\d+)\])$")

# JavaScript that defines __findElement(selector).
# Ported from actionbook CLI session.rs — supports CSS, XPath, and @eN refs.
_FIND_ELEMENT_JS = r"""
function __findElement(selector) {
    // Normalize [ref=eN] format to @eN
    const refMatch = selector.match(/^\[ref=(e\d+)\]$/);
    if (refMatch) selector = '@' + refMatch[1];
    if (/^@e\d+$/.test(selector)) {
        const targetNum = parseInt(selector.slice(2));
        const SKIP_TAGS = new Set(['script','style','noscript','template','svg','path','defs','clippath','lineargradient','stop','meta','link','br','wbr']);
        const INLINE_TAGS = new Set(['strong','b','em','i','code','span','small','sup','sub','abbr','mark','u','s','del','ins','time','q','cite','dfn','var','samp','kbd']);
        const INTERACTIVE_ROLES = new Set(['button','link','textbox','checkbox','radio','combobox','listbox','menuitem','menuitemcheckbox','menuitemradio','option','searchbox','slider','spinbutton','switch','tab','treeitem']);
        const CONTENT_ROLES = new Set(['heading','cell','gridcell','columnheader','rowheader','listitem','article','region','main','navigation','img']);
        function getRole(el) {
            const explicit = el.getAttribute('role');
            if (explicit) return explicit.toLowerCase();
            const tag = el.tagName.toLowerCase();
            if (INLINE_TAGS.has(tag)) return tag;
            const roleMap = {
                'a': el.hasAttribute('href') ? 'link' : 'generic',
                'button': 'button', 'input': getInputRole(el), 'select': 'combobox', 'textarea': 'textbox', 'img': 'img',
                'h1':'heading','h2':'heading','h3':'heading','h4':'heading','h5':'heading','h6':'heading',
                'nav':'navigation','main':'main','header':'banner','footer':'contentinfo','aside':'complementary',
                'form':'form','table':'table','thead':'rowgroup','tbody':'rowgroup','tfoot':'rowgroup',
                'tr':'row','th':'columnheader','td':'cell','ul':'list','ol':'list','li':'listitem',
                'details':'group','summary':'button','dialog':'dialog',
                'section': el.hasAttribute('aria-label') || el.hasAttribute('aria-labelledby') ? 'region' : 'generic',
                'article':'article'
            };
            return roleMap[tag] || 'generic';
        }
        function getInputRole(el) {
            const type = (el.getAttribute('type') || 'text').toLowerCase();
            const map = {'text':'textbox','email':'textbox','password':'textbox','search':'searchbox','tel':'textbox','url':'textbox','number':'spinbutton','checkbox':'checkbox','radio':'radio','submit':'button','reset':'button','button':'button','range':'slider'};
            return map[type] || 'textbox';
        }
        function getAccessibleName(el) {
            const ariaLabel = el.getAttribute('aria-label');
            if (ariaLabel) return ariaLabel.trim();
            const labelledBy = el.getAttribute('aria-labelledby');
            if (labelledBy) { const label = document.getElementById(labelledBy); if (label) return label.textContent?.trim()?.substring(0, 100) || ''; }
            const tag = el.tagName.toLowerCase();
            if (tag === 'img') return el.getAttribute('alt') || '';
            if (tag === 'input' || tag === 'textarea' || tag === 'select') {
                if (el.id) { const label = document.querySelector('label[for="' + el.id + '"]'); if (label) return label.textContent?.trim()?.substring(0, 100) || ''; }
                return el.getAttribute('placeholder') || el.getAttribute('title') || '';
            }
            if (tag === 'a' || tag === 'button' || tag === 'summary') return '';
            if (['h1','h2','h3','h4','h5','h6'].includes(tag)) return el.textContent?.trim()?.substring(0, 150) || '';
            const title = el.getAttribute('title');
            if (title) return title.trim();
            return '';
        }
        function isHidden(el) {
            if (el.hidden) return true;
            if (el.getAttribute('aria-hidden') === 'true') return true;
            const style = el.style;
            if (style.display === 'none' || style.visibility === 'hidden') return true;
            if (el.offsetParent === null && el.tagName.toLowerCase() !== 'body' && getComputedStyle(el).position !== 'fixed' && getComputedStyle(el).position !== 'sticky') {
                const cs = getComputedStyle(el);
                if (cs.display === 'none' || cs.visibility === 'hidden') return true;
            }
            return false;
        }
        let refCounter = 0;
        function walkFind(el, depth) {
            if (depth > 15) return null;
            const tag = el.tagName.toLowerCase();
            if (SKIP_TAGS.has(tag)) return null;
            if (isHidden(el)) return null;
            const role = getRole(el);
            const name = getAccessibleName(el);
            const isInteractive = INTERACTIVE_ROLES.has(role);
            const isContent = CONTENT_ROLES.has(role);
            const shouldRef = isInteractive || (isContent && name);
            if (shouldRef) {
                refCounter++;
                if (refCounter === targetNum) return el;
            }
            for (const child of el.children) {
                const found = walkFind(child, depth + 1);
                if (found) return found;
            }
            return null;
        }
        return walkFind(document.body, 0);
    }
    if (selector.startsWith('//') || selector.startsWith('(//')) {
        const result = document.evaluate(selector, document, null, XPathResult.FIRST_ORDERED_NODE_TYPE, null);
        return result.singleNodeValue;
    }
    return document.querySelector(selector);
}
"""


def _is_ref_selector(selector: str) -> bool:
    """Check if a selector is a snapshot ref (@eN or [ref=eN])."""
    return bool(_REF_RE.match(selector.strip()))


def _is_js_selector(selector: str) -> bool:
    """Check if a selector needs JS-based resolution (ref or XPath)."""
    s = selector.strip()
    return _is_ref_selector(s) or s.startswith("//") or s.startswith("(//")


def _send(payload: dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(payload, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def _result(req_id: int, value: Any) -> None:
    _send({"id": req_id, "ok": True, "result": value})


def _error(req_id: int, exc: BaseException) -> None:
    _send({
        "id": req_id,
        "ok": False,
        "error": f"{type(exc).__name__}: {exc}",
    })


def _get_active_page(browser):
    contexts = browser.contexts
    if not contexts:
        context = browser.new_context()
        return context.new_page()
    context = contexts[0]
    pages = context.pages
    return pages[0] if pages else context.new_page()


def _js_click(page, selector: str) -> None:
    """Click via JS element resolution + coordinate-based mouse click."""
    sel_json = json.dumps(selector)
    js = (
        "(function() {\n"
        + _FIND_ELEMENT_JS
        + f"\nconst el = __findElement({sel_json});\n"
        "if (!el) return null;\n"
        "el.scrollIntoView({ behavior: 'instant', block: 'center', inline: 'center' });\n"
        "const rect = el.getBoundingClientRect();\n"
        "return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };\n"
        "})()"
    )
    coords = page.evaluate(js)
    if coords is None:
        raise ValueError(f"Element not found for selector: {selector}")
    page.mouse.click(coords["x"], coords["y"])


def _js_fill(page, selector: str, text: str) -> None:
    """Fill via JS element resolution — focus, set value, dispatch events."""
    sel_json = json.dumps(selector)
    text_json = json.dumps(text)
    js = (
        "(function() {\n"
        + _FIND_ELEMENT_JS
        + f"\nconst el = __findElement({sel_json});\n"
        "if (!el) return false;\n"
        "el.focus();\n"
        f"el.value = {text_json};\n"
        "el.dispatchEvent(new Event('input', { bubbles: true }));\n"
        "el.dispatchEvent(new Event('change', { bubbles: true }));\n"
        "return true;\n"
        "})()"
    )
    ok = page.evaluate(js)
    if not ok:
        raise ValueError(f"Element not found for selector: {selector}")


def _js_type(page, selector: str, text: str) -> None:
    """Type via JS element resolution — focus then use keyboard."""
    sel_json = json.dumps(selector)
    js = (
        "(function() {\n"
        + _FIND_ELEMENT_JS
        + f"\nconst el = __findElement({sel_json});\n"
        "if (!el) return false;\n"
        "el.focus();\n"
        "return true;\n"
        "})()"
    )
    ok = page.evaluate(js)
    if not ok:
        raise ValueError(f"Element not found for selector: {selector}")
    page.keyboard.type(text)


def _js_hover(page, selector: str) -> None:
    """Hover via JS element resolution + coordinate-based mouse move."""
    sel_json = json.dumps(selector)
    js = (
        "(function() {\n"
        + _FIND_ELEMENT_JS
        + f"\nconst el = __findElement({sel_json});\n"
        "if (!el) return null;\n"
        "el.scrollIntoView({ behavior: 'instant', block: 'center', inline: 'center' });\n"
        "const rect = el.getBoundingClientRect();\n"
        "return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };\n"
        "})()"
    )
    coords = page.evaluate(js)
    if coords is None:
        raise ValueError(f"Element not found for selector: {selector}")
    page.mouse.move(coords["x"], coords["y"])


def _js_select_option(page, selector: str, value: str) -> list[str]:
    """Select option via JS element resolution."""
    sel_json = json.dumps(selector)
    val_json = json.dumps(value)
    js = (
        "(function() {\n"
        + _FIND_ELEMENT_JS
        + f"\nconst el = __findElement({sel_json});\n"
        "if (!el || el.tagName.toLowerCase() !== 'select') return null;\n"
        f"const val = {val_json};\n"
        "for (const opt of el.options) {\n"
        "  if (opt.value === val) {\n"
        "    opt.selected = true;\n"
        "    el.dispatchEvent(new Event('change', { bubbles: true }));\n"
        "    return [opt.value];\n"
        "  }\n"
        "}\n"
        "return [];\n"
        "})()"
    )
    result = page.evaluate(js)
    if result is None:
        raise ValueError(f"Element not found for selector: {selector}")
    return result


def _js_wait_for_selector(page, selector: str, timeout: float | None) -> None:
    """Wait until a JS-resolved selector appears, honoring timeout."""
    sel_json = json.dumps(selector)
    js = (
        "(function() {\n"
        + _FIND_ELEMENT_JS
        + f"\nreturn __findElement({sel_json}) !== null;\n"
        "})()"
    )
    page.wait_for_function(js, timeout=timeout)


def main() -> int:
    if len(sys.argv) < 3:
        _send({"ok": False, "error": "Usage: playwright_worker.py <ws_endpoint> <timeout_ms>"})
        return 2

    ws_endpoint = sys.argv[1]
    timeout_ms = float(sys.argv[2])

    pw = None
    browser = None
    page = None
    try:
        pw = sync_playwright().start()
        browser = pw.chromium.connect_over_cdp(ws_endpoint, timeout=timeout_ms)
        page = _get_active_page(browser)
        _send({"ok": True, "ready": True})
    except BaseException as exc:
        _send({"ok": False, "error": f"{type(exc).__name__}: {exc}"})
        if browser is not None:
            try:
                browser.close()
            except Exception:
                pass
        if pw is not None:
            try:
                pw.stop()
            except Exception:
                pass
        return 1

    assert page is not None

    try:
        for line in sys.stdin:
            line = line.strip()
            if not line:
                continue
            try:
                payload = json.loads(line)
            except json.JSONDecodeError as exc:
                _send({"ok": False, "error": f"JSONDecodeError: {exc}"})
                continue

            req_id = int(payload.get("id", 0))
            op = payload.get("op")
            params = payload.get("params") or {}

            try:
                if op == "close":
                    _result(req_id, "closing")
                    break
                if op == "url":
                    _result(req_id, page.url)
                elif op == "title":
                    _result(req_id, page.title())
                elif op == "goto":
                    page.goto(
                        params["url"],
                        timeout=params.get("timeout"),
                        wait_until=params.get("wait_until"),
                    )
                    _result(req_id, None)
                elif op == "wait_for_selector":
                    selector = params["selector"]
                    if _is_js_selector(selector):
                        _js_wait_for_selector(
                            page,
                            selector,
                            timeout=params.get("timeout"),
                        )
                    else:
                        page.wait_for_selector(
                            selector,
                            timeout=params.get("timeout"),
                        )
                    _result(req_id, None)
                elif op == "click":
                    selector = params["selector"]
                    if _is_js_selector(selector):
                        _js_click(page, selector)
                    else:
                        page.click(selector)
                    _result(req_id, None)
                elif op == "type":
                    selector = params["selector"]
                    if _is_js_selector(selector):
                        _js_type(page, selector, params["text"])
                    else:
                        page.type(selector, params["text"])
                    _result(req_id, None)
                elif op == "fill":
                    selector = params["selector"]
                    if _is_js_selector(selector):
                        _js_fill(page, selector, params["text"])
                    else:
                        page.fill(selector, params["text"])
                    _result(req_id, None)
                elif op == "select_option":
                    selector = params["selector"]
                    if _is_js_selector(selector):
                        _result(req_id, _js_select_option(page, selector, params["value"]))
                    else:
                        _result(
                            req_id,
                            page.select_option(selector, value=params["value"]),
                        )
                elif op == "keyboard_press":
                    page.keyboard.press(params["key"])
                    _result(req_id, None)
                elif op == "hover":
                    selector = params["selector"]
                    if _is_js_selector(selector):
                        _js_hover(page, selector)
                    else:
                        page.hover(selector)
                    _result(req_id, None)
                elif op == "inner_text":
                    _result(req_id, page.inner_text(params["selector"]))
                elif op == "inner_html":
                    _result(req_id, page.inner_html(params["selector"]))
                elif op == "content":
                    _result(req_id, page.content())
                elif op == "wait_for_load_state":
                    page.wait_for_load_state(
                        params["state"],
                        timeout=params.get("timeout"),
                    )
                    _result(req_id, None)
                elif op == "go_back":
                    page.go_back()
                    _result(req_id, None)
                elif op == "go_forward":
                    page.go_forward()
                    _result(req_id, None)
                elif op == "reload":
                    page.reload(wait_until=params.get("wait_until"))
                    _result(req_id, None)
                elif op == "evaluate":
                    _result(req_id, page.evaluate(params["expression"]))
                else:
                    raise ValueError(f"Unknown op: {op}")
            except BaseException as exc:
                _error(req_id, exc)
    finally:
        try:
            browser.close()
        except Exception:
            pass
        try:
            pw.stop()  # type: ignore[union-attr]
        except Exception:
            pass

    return 0


if __name__ == "__main__":
    raise SystemExit(main())

/// Stealth JS injected at document start via Page.addScriptToEvaluateOnNewDocument.
///
/// Covers:
///  - navigator.webdriver removal
///  - dynamic cdc_* marker cleanup
///  - Playwright/Puppeteer trace removal
///  - chrome object (runtime, loadTimes with realistic fields, csi, app)
///  - navigator: hardwareConcurrency, deviceMemory, language, languages, platform, vendor, maxTouchPoints
///  - navigator.plugins (PDF + Chrome PDF Viewer only, no NaCl)
///  - navigator.permissions.query (spoofs notification/camera/microphone)
///  - WebGL renderer/vendor strings (parameterised via format!())
///  - screen.colorDepth / screen.pixelDepth
///  - Canvas fingerprint noise (toDataURL/toBlob)
///  - Idempotency guard so multiple calls are safe
pub fn stealth_js(webgl_vendor: &str, webgl_renderer: &str) -> String {
    format!(
        r#"(function() {{
if (Navigator.prototype._s) {{ return; }}
Object.defineProperty(Navigator.prototype, '_s', {{ value: 1, configurable: false, enumerable: false }});

// 1. navigator.webdriver — delete from prototype so 'webdriver' in navigator === false
// Do NOT re-define after deleting — re-defining would re-create the property.
try {{ delete Navigator.prototype.webdriver; }} catch(e) {{}}

// 2. cdc_ marker cleanup (Selenium/ChromeDriver artifacts)
Object.keys(window)
  .filter(k => k.startsWith('cdc_') || k.startsWith("cdc_"))
  .forEach(k => {{ try {{ delete window[k]; }} catch(e) {{}} }});

// 2b. Playwright / Puppeteer trace removal
try {{ delete window.__playwright; }} catch(e) {{}}
try {{ delete window.__pw_manual; }} catch(e) {{}}
try {{ delete window.__PW_inspect; }} catch(e) {{}}

// 3. window.chrome
if (!window.chrome) {{
  window.chrome = {{
    runtime: {{}},
    loadTimes: function() {{
      const now = Date.now() / 1000;
      return {{
        requestTime: now - 0.5 - Math.random() * 0.1,
        startLoadTime: now - 0.4 - Math.random() * 0.05,
        commitLoadTime: now - 0.2 - Math.random() * 0.05,
        finishDocumentLoadTime: now - 0.05 - Math.random() * 0.02,
        finishLoadTime: now - 0.02 - Math.random() * 0.01,
        firstPaintTime: now - 0.15 - Math.random() * 0.05,
        firstPaintAfterLoadTime: 0,
        navigationType: 'Other',
        wasFetchedViaSpdy: false,
        wasNpnNegotiated: false,
        npnNegotiatedProtocol: 'unknown',
        wasAlternateProtocolAvailable: false,
        connectionInfo: 'http/1.1'
      }};
    }},
    csi: function() {{ return {{}}; }},
    app: {{}},
  }};
}}

// 4. navigator props
const nav = navigator;
try {{ Object.defineProperty(nav, 'hardwareConcurrency', {{ get: () => 8 }}); }} catch(e) {{}}
try {{ Object.defineProperty(nav, 'deviceMemory', {{ get: () => 8 }}); }} catch(e) {{}}
try {{ Object.defineProperty(nav, 'language', {{ get: () => 'en-US' }}); }} catch(e) {{}}
try {{ Object.defineProperty(nav, 'languages', {{ get: () => ['en-US', 'en'] }}); }} catch(e) {{}}
// platform: do NOT override — must match User-Agent OS (macOS=MacIntel, Windows=Win32, Linux=Linux x86_64)
try {{ Object.defineProperty(nav, 'vendor', {{ get: () => 'Google Inc.' }}); }} catch(e) {{}}
try {{ Object.defineProperty(nav, 'maxTouchPoints', {{ get: () => 0 }}); }} catch(e) {{}}

// 5. navigator.plugins (PDF only, no NaCl)
// Use the real PluginArray/Plugin/MimeType prototypes so instanceof checks pass
try {{
  const realPlugins = navigator.plugins;
  const makePlugin = (name, filename, desc, mimeType, mimeDesc) => {{
    const mt = {{ type: mimeType, description: mimeDesc, suffixes: '' }};
    Object.setPrototypeOf(mt, MimeType.prototype);
    const p = {{ name, filename, description: desc, length: 1, 0: mt, item: i => i === 0 ? mt : null, namedItem: n => n === mimeType ? mt : null }};
    Object.setPrototypeOf(p, Plugin.prototype);
    mt.enabledPlugin = p;
    return p;
  }};
  const p0 = makePlugin('PDF Viewer', 'internal-pdf-viewer', 'Portable Document Format', 'application/pdf', 'Portable Document Format');
  const p1 = makePlugin('Chrome PDF Viewer', 'internal-pdf-viewer', 'Portable Document Format', 'application/x-google-chrome-pdf', 'Portable Document Format');
  const plist = {{ 0: p0, 1: p1, length: 2, item: i => [p0, p1][i], namedItem: n => [p0, p1].find(p => p.name === n) || null, refresh: () => {{}} }};
  Object.setPrototypeOf(plist, PluginArray.prototype);
  Object.defineProperty(navigator, 'plugins', {{ get: () => plist }});
}} catch(e) {{}}

// 6. navigator.permissions (spoof notification/camera/microphone state)
try {{
  const origQuery = navigator.permissions.query.bind(navigator.permissions);
  navigator.permissions.query = (params) => {{
    if (['notifications', 'camera', 'microphone'].includes(params.name)) {{
      return Promise.resolve({{ state: 'prompt', onchange: null }});
    }}
    return origQuery(params);
  }};
}} catch(e) {{}}

// 7. WebGL vendor/renderer (v1 + v2)
try {{
  const getParam = WebGLRenderingContext.prototype.getParameter;
  WebGLRenderingContext.prototype.getParameter = function(param) {{
    if (param === 37445) return '{webgl_vendor}';
    if (param === 37446) return '{webgl_renderer}';
    return getParam.call(this, param);
  }};
  if (typeof WebGL2RenderingContext !== 'undefined') {{
    const getParam2 = WebGL2RenderingContext.prototype.getParameter;
    WebGL2RenderingContext.prototype.getParameter = function(param) {{
      if (param === 37445) return '{webgl_vendor}';
      if (param === 37446) return '{webgl_renderer}';
      return getParam2.call(this, param);
    }};
  }}
}} catch(e) {{}}

// 8. Screen properties (colorDepth/pixelDepth — ensures consistency in headless)
try {{ Object.defineProperty(screen, 'colorDepth', {{ get: () => 24 }}); }} catch(e) {{}}
try {{ Object.defineProperty(screen, 'pixelDepth', {{ get: () => 24 }}); }} catch(e) {{}}

// 9. Canvas fingerprint noise — use a temporary offscreen canvas to avoid mutating the original.
//    Only applies noise when a 2D context is already active (never poisons a WebGL canvas).
try {{
  const _ctxMap = new WeakMap();
  const _origGetContext = HTMLCanvasElement.prototype.getContext;
  HTMLCanvasElement.prototype.getContext = function(type, ...rest) {{
    const ctx = _origGetContext.call(this, type, ...rest);
    if (ctx && type === '2d') {{ _ctxMap.set(this, ctx); }}
    return ctx;
  }};

  function _addNoise(srcCanvas) {{
    const ctx = _ctxMap.get(srcCanvas);
    if (!ctx || srcCanvas.width === 0 || srcCanvas.height === 0) return null;
    try {{
      const tmp = document.createElement('canvas');
      tmp.width = srcCanvas.width;
      tmp.height = srcCanvas.height;
      const tmpCtx = _origGetContext.call(tmp, '2d');
      tmpCtx.drawImage(srcCanvas, 0, 0);
      const img = tmpCtx.getImageData(0, 0, tmp.width, tmp.height);
      for (let i = 0; i < img.data.length; i += 40) {{
        const v = img.data[i];
        img.data[i] = v > 0 && v < 255 ? v + (Math.random() < 0.5 ? -1 : 1) : v;
      }}
      tmpCtx.putImageData(img, 0, 0);
      return tmp;
    }} catch(e2) {{ return null; }}
  }}

  const _toDataURL = HTMLCanvasElement.prototype.toDataURL;
  HTMLCanvasElement.prototype.toDataURL = function(...args) {{
    const tmp = _addNoise(this);
    return tmp ? _toDataURL.apply(tmp, args) : _toDataURL.apply(this, args);
  }};
  const _toBlob = HTMLCanvasElement.prototype.toBlob;
  HTMLCanvasElement.prototype.toBlob = function(...args) {{
    const tmp = _addNoise(this);
    return tmp ? _toBlob.apply(tmp, args) : _toBlob.apply(this, args);
  }};
}} catch(e) {{}}

}})();"#,
        webgl_vendor = webgl_vendor,
        webgl_renderer = webgl_renderer,
    )
}

/// Default stealth JS with common Windows/Intel WebGL strings.
pub static STEALTH_JS: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| stealth_js("Intel Inc.", "Intel Iris OpenGL Engine"));

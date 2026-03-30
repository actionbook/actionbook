/// Stealth JS injected at document start via Page.addScriptToEvaluateOnNewDocument.
///
/// Covers:
///  - navigator.webdriver removal
///  - dynamic cdc_* marker cleanup
///  - chrome object (runtime, loadTimes, csi, app)
///  - navigator: hardwareConcurrency, deviceMemory, language, languages, platform, vendor
///  - navigator.plugins (PDF + Chrome PDF Viewer only, no NaCl)
///  - navigator.permissions.query (spoofs notification/camera/microphone)
///  - WebGL renderer/vendor strings (parameterised via format!())
///  - Idempotency guard so multiple calls are safe
pub fn stealth_js(webgl_vendor: &str, webgl_renderer: &str) -> String {
    format!(
        r#"(function() {{
if (window.__stealth_applied__) {{ return; }}
window.__stealth_applied__ = true;

// 1. navigator.webdriver
Object.defineProperty(navigator, 'webdriver', {{
  get: () => undefined,
  configurable: true,
}});

// 2. cdc_ marker cleanup (Selenium/ChromeDriver artifacts)
Object.keys(window)
  .filter(k => k.startsWith('cdc_') || k.startsWith("cdc_"))
  .forEach(k => {{ try {{ delete window[k]; }} catch(e) {{}} }});

// 3. window.chrome
if (!window.chrome) {{
  window.chrome = {{
    runtime: {{}},
    loadTimes: function() {{ return {{}}; }},
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
try {{ Object.defineProperty(nav, 'platform', {{ get: () => 'Win32' }}); }} catch(e) {{}}
try {{ Object.defineProperty(nav, 'vendor', {{ get: () => 'Google Inc.' }}); }} catch(e) {{}}

// 5. navigator.plugins (PDF only, no NaCl)
try {{
  const makePlugin = (name, filename, desc, mimeType, mimeDesc) => {{
    const mt = {{ type: mimeType, description: mimeDesc, suffixes: '' }};
    const p = {{ name, filename, description: desc, length: 1, 0: mt, item: i => i === 0 ? mt : null, namedItem: n => n === mimeType ? mt : null }};
    mt.enabledPlugin = p;
    return p;
  }};
  const plugins = [
    makePlugin('PDF Viewer', 'internal-pdf-viewer', 'Portable Document Format', 'application/pdf', 'Portable Document Format'),
    makePlugin('Chrome PDF Viewer', 'internal-pdf-viewer', 'Portable Document Format', 'application/x-google-chrome-pdf', 'Portable Document Format'),
  ];
  const plist = Object.assign(plugins, {{
    length: plugins.length,
    item: i => plugins[i],
    namedItem: n => plugins.find(p => p.name === n) || null,
    refresh: () => {{}},
  }});
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

// 7. WebGL vendor/renderer
try {{
  const getParam = WebGLRenderingContext.prototype.getParameter;
  WebGLRenderingContext.prototype.getParameter = function(param) {{
    if (param === 37445) return '{webgl_vendor}';
    if (param === 37446) return '{webgl_renderer}';
    return getParam.call(this, param);
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

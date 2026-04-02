use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::element::TabContext;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// Output file path
    pub path: String,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
    /// Capture full page (not just the current viewport)
    #[arg(long)]
    #[serde(default)]
    pub full: bool,
    /// Overlay numbered labels on interactive elements ([N] = @eN)
    #[arg(long)]
    #[serde(default)]
    pub annotate: bool,
    /// JPEG quality (0-100, effective only for jpeg format)
    #[arg(long)]
    pub screenshot_quality: Option<u8>,
    /// Image format (png or jpeg)
    #[arg(long)]
    pub screenshot_format: Option<String>,
    /// CSS selector to limit capture region
    #[arg(long)]
    pub selector: Option<String>,
}

pub const COMMAND_NAME: &str = "browser screenshot";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    if let ActionResult::Fatal { code, .. } = result
        && code == "SESSION_NOT_FOUND"
    {
        return None;
    }
    let tab_id = if let ActionResult::Fatal { code, .. } = result
        && code == "TAB_NOT_FOUND"
    {
        None
    } else {
        Some(cmd.tab.clone())
    };
    let (url, title) = match result {
        ActionResult::Ok { data } => {
            let url = data
                .get("__ctx_url")
                .and_then(|v| v.as_str())
                .map(String::from);
            let title = data
                .get("__ctx_title")
                .and_then(|v| v.as_str())
                .map(String::from);
            (url, title)
        }
        _ => (None, None),
    };
    Some(ResponseContext {
        session_id: cmd.session.clone(),
        tab_id,
        window_id: None,
        url,
        title,
    })
}

/// Infer image format from file extension.
fn infer_format(path: &str) -> &'static str {
    match std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg" | "jpeg") => "jpeg",
        _ => "png",
    }
}

/// Return MIME type for a CDP format string.
fn mime_type(format: &str) -> &'static str {
    if format == "jpeg" {
        "image/jpeg"
    } else {
        "image/png"
    }
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    // Resolve session + tab
    let mut ctx = match TabContext::new(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    let url = crate::browser::navigation::get_tab_url(&ctx.cdp, &ctx.target_id).await;
    let title = crate::browser::navigation::get_tab_title(&ctx.cdp, &ctx.target_id).await;

    // Determine format
    let format = match cmd.screenshot_format.as_deref() {
        Some("jpeg" | "jpg") => "jpeg",
        Some("png") => "png",
        Some(unknown) => {
            return ActionResult::fatal(
                "INVALID_ARGUMENT",
                format!("unsupported screenshot format: '{unknown}' (supported: png, jpeg)"),
            );
        }
        None => infer_format(&cmd.path),
    };

    // Validate --full + --selector mutual exclusion
    if cmd.full && cmd.selector.is_some() {
        return ActionResult::fatal(
            "INVALID_ARGUMENT",
            "--full and --selector are mutually exclusive",
        );
    }

    // ── Annotate: collect rects, inject overlay ───────────────────
    let mut overlay_injected = false;
    let mut annotation_items: Vec<AnnotationItem> = Vec::new();

    if cmd.annotate {
        let ref_cache = {
            let mut reg = ctx.registry().lock().await;
            reg.take_ref_cache(&cmd.session, &cmd.tab)
        };

        annotation_items = collect_annotation_rects(&ctx.cdp, &ctx.target_id, &ref_cache).await;

        // Put ref_cache back immediately
        {
            let mut reg = ctx.registry().lock().await;
            reg.put_ref_cache(&cmd.session, &cmd.tab, ref_cache);
        }

        // Filter by selector region if applicable
        if let Some(ref sel) = cmd.selector
            && let Ok((_, target_rect)) = get_selector_rect(&mut ctx, sel).await
        {
            annotation_items = filter_annotations(annotation_items, Some(&target_rect));
        }

        if !annotation_items.is_empty()
            && inject_overlay(&ctx.cdp, &ctx.target_id, &annotation_items)
                .await
                .is_ok()
        {
            overlay_injected = true;
        }
    }

    // ── Build CDP params ─────────────────────────────────────────
    let mut params = json!({
        "format": format,
        "fromSurface": true,
    });

    if format == "jpeg" {
        let quality = cmd.screenshot_quality.unwrap_or(80);
        params["quality"] = json!(quality);
    }

    // Full page: get layout metrics for clip
    if cmd.full {
        if let Ok(metrics) = ctx
            .cdp
            .execute_on_tab(&ctx.target_id, "Page.getLayoutMetrics", json!({}))
            .await
        {
            let content_size = metrics["result"]
                .get("contentSize")
                .or_else(|| metrics["result"].get("cssContentSize"));
            if let Some(size) = content_size {
                let width = size.get("width").and_then(|v| v.as_f64()).unwrap_or(1280.0);
                let height = size.get("height").and_then(|v| v.as_f64()).unwrap_or(720.0);
                params["clip"] = json!({
                    "x": 0,
                    "y": 0,
                    "width": width,
                    "height": height,
                    "scale": 1,
                });
                params["captureBeyondViewport"] = json!(true);
            }
        }
    } else if let Some(ref sel) = cmd.selector {
        // Clip to selector region
        match get_selector_rect(&mut ctx, sel).await {
            Ok((clip, _)) => {
                params["clip"] = clip;
            }
            Err(e) => {
                if overlay_injected {
                    let _ = remove_overlay(&ctx.cdp, &ctx.target_id).await;
                }
                return e;
            }
        }
    }

    // ── Capture screenshot ───────────────────────────────────────
    let capture_result = ctx
        .cdp
        .execute_on_tab(&ctx.target_id, "Page.captureScreenshot", params)
        .await;

    // Always clean up overlay
    if overlay_injected {
        let _ = remove_overlay(&ctx.cdp, &ctx.target_id).await;
    }

    let resp = match capture_result {
        Ok(v) => v,
        Err(e) => {
            return crate::daemon::cdp_session::cdp_error_to_result(e, "INTERNAL_ERROR");
        }
    };

    let base64_data = resp["result"]["data"].as_str().unwrap_or("");
    if base64_data.is_empty() {
        return ActionResult::fatal("INTERNAL_ERROR", "CDP returned empty screenshot data");
    }

    // ── Decode + write file ──────────────────────────────────────
    use base64::Engine;
    let bytes = match base64::engine::general_purpose::STANDARD.decode(base64_data) {
        Ok(b) => b,
        Err(e) => {
            return ActionResult::fatal("INTERNAL_ERROR", format!("base64 decode failed: {e}"));
        }
    };

    // Resolve to absolute path
    let abs_path = std::path::Path::new(&cmd.path);
    let abs_path = if abs_path.is_absolute() {
        abs_path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(abs_path)
    };
    let path_str = abs_path.to_string_lossy().to_string();

    if let Err(e) = std::fs::write(&abs_path, &bytes) {
        return ActionResult::Fatal {
            code: "ARTIFACT_WRITE_FAILED".to_string(),
            message: format!("failed to write screenshot to {path_str}: {e}"),
            hint: String::new(),
            details: Some(json!({ "path": path_str })),
        };
    }

    // ── Build annotations data ───────────────────────────────────
    let mut data = json!({
        "artifact": {
            "path": path_str,
            "mime_type": mime_type(format),
            "bytes": bytes.len(),
        },
        "__ctx_url": url,
        "__ctx_title": title,
    });

    if cmd.annotate {
        // Project annotations to screenshot coordinates
        let scroll = if cmd.full {
            get_scroll_offsets(&ctx.cdp, &ctx.target_id).await.ok()
        } else {
            None
        };

        let selector_rect = if let Some(ref sel) = cmd.selector {
            get_selector_rect(&mut ctx, sel).await.ok().map(|(_, r)| r)
        } else {
            None
        };

        let annotations = project_annotations(&annotation_items, selector_rect.as_ref(), scroll);
        data["annotations"] = json!(annotations);
    }

    ActionResult::ok(data)
}

// ── Annotation types and helpers ─────────────────────────────────────

#[derive(Debug, Clone)]
struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Debug, Clone)]
struct AnnotationItem {
    ref_id: String,
    number: u64,
    role: String,
    name: String,
    rect: Rect,
}

const OVERLAY_ID: &str = "__ab_screenshot_annotations__";

/// Resolve all RefCache entries to screen positions via CDP.
/// Frame-aware: routes DOM.resolveNode to the correct CDP session for iframe elements.
async fn collect_annotation_rects(
    cdp: &crate::daemon::cdp_session::CdpSession,
    target_id: &str,
    ref_cache: &super::snapshot_transform::RefCache,
) -> Vec<AnnotationItem> {
    let entries: Vec<_> = ref_cache
        .entries()
        .map(|(bid, entry)| (bid, entry.clone()))
        .collect();

    if entries.is_empty() {
        return Vec::new();
    }

    // Batch resolve backendNodeId → objectId (frame-aware)
    let mut resolved: Vec<(super::snapshot_transform::RefEntry, String)> = Vec::new();
    for (backend_node_id, entry) in &entries {
        let resp = crate::browser::element::execute_for_frame(
            cdp,
            target_id,
            entry.frame_id.as_deref(),
            "DOM.resolveNode",
            json!({
                "backendNodeId": backend_node_id,
                "objectGroup": "ab-annotate"
            }),
        )
        .await;
        if let Ok(val) = resp
            && let Some(oid) = val
                .pointer("/result/object/objectId")
                .and_then(|v| v.as_str())
        {
            resolved.push((entry.clone(), oid.to_string()));
        }
    }

    // Batch get bounding rects (frame-aware for iframe elements)
    let mut items = Vec::new();
    for (entry, object_id) in &resolved {
        if let Some(rect) =
            get_rect_for_object(cdp, target_id, object_id, entry.frame_id.as_deref()).await
            && rect.width > 0.0
            && rect.height > 0.0
        {
            let number = entry
                .ref_id
                .strip_prefix('e')
                .and_then(|n| n.parse::<u64>().ok())
                .unwrap_or(0);
            items.push(AnnotationItem {
                ref_id: entry.ref_id.clone(),
                number,
                role: entry.role.clone(),
                name: entry.name.clone(),
                rect,
            });
        }
    }

    items.sort_by_key(|item| item.number);
    items
}

async fn get_rect_for_object(
    cdp: &crate::daemon::cdp_session::CdpSession,
    target_id: &str,
    object_id: &str,
    frame_id: Option<&str>,
) -> Option<Rect> {
    let resp = crate::browser::element::execute_for_frame(
        cdp,
        target_id,
        frame_id,
        "Runtime.callFunctionOn",
        json!({
            "functionDeclaration": "function() { const r = this.getBoundingClientRect(); return {x: r.x, y: r.y, width: r.width, height: r.height}; }",
            "objectId": object_id,
            "returnByValue": true,
            "awaitPromise": false,
        }),
    )
    .await
    .ok()?;

    let value = &resp["result"]["result"]["value"];
    Some(Rect {
        x: value.get("x")?.as_f64()?,
        y: value.get("y")?.as_f64()?,
        width: value.get("width")?.as_f64()?,
        height: value.get("height")?.as_f64()?,
    })
}

fn filter_annotations(items: Vec<AnnotationItem>, target: Option<&Rect>) -> Vec<AnnotationItem> {
    let mut filtered: Vec<_> = items
        .into_iter()
        .filter(|item| match target {
            Some(t) => overlaps(&item.rect, t),
            None => true,
        })
        .collect();
    filtered.sort_by_key(|item| item.number);
    filtered
}

fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.x < b.x + b.width && a.x + a.width > b.x && a.y < b.y + b.height && a.y + a.height > b.y
}

async fn inject_overlay(
    cdp: &crate::daemon::cdp_session::CdpSession,
    target_id: &str,
    items: &[AnnotationItem],
) -> Result<(), ActionResult> {
    let overlay_data: Vec<_> = items
        .iter()
        .map(|item| {
            json!({
                "number": item.number,
                "x": item.rect.x.round() as i64,
                "y": item.rect.y.round() as i64,
                "width": item.rect.width.round() as i64,
                "height": item.rect.height.round() as i64,
            })
        })
        .collect();

    let expression = format!(
        r#"(() => {{
            var items = {items};
            var id = {overlay_id};
            var existing = document.getElementById(id);
            if (existing) existing.remove();
            var sx = window.scrollX || 0;
            var sy = window.scrollY || 0;
            var c = document.createElement('div');
            c.id = id;
            c.style.cssText = 'position:absolute;top:0;left:0;width:0;height:0;pointer-events:none;z-index:2147483647;';
            for (var i = 0; i < items.length; i++) {{
                var it = items[i];
                var dx = it.x + sx;
                var dy = it.y + sy;
                var b = document.createElement('div');
                b.style.cssText = 'position:absolute;left:' + dx + 'px;top:' + dy + 'px;width:' + it.width + 'px;height:' + it.height + 'px;border:2px solid rgba(255,0,0,0.8);box-sizing:border-box;pointer-events:none;';
                var l = document.createElement('div');
                l.textContent = String(it.number);
                var labelTop = dy < 14 ? '2px' : '-14px';
                l.style.cssText = 'position:absolute;top:' + labelTop + ';left:-2px;background:rgba(255,0,0,0.9);color:#fff;font:bold 11px/14px monospace;padding:0 4px;border-radius:2px;white-space:nowrap;';
                b.appendChild(l);
                c.appendChild(b);
            }}
            document.documentElement.appendChild(c);
            return true;
        }})()"#,
        items = serde_json::to_string(&overlay_data).unwrap_or_else(|_| "[]".to_string()),
        overlay_id = serde_json::to_string(OVERLAY_ID).unwrap_or_else(|_| "\"\"".to_string()),
    );

    cdp.execute_on_tab(
        target_id,
        "Runtime.evaluate",
        json!({
            "expression": expression,
            "returnByValue": true,
            "awaitPromise": false,
        }),
    )
    .await
    .map_err(|e| crate::daemon::cdp_session::cdp_error_to_result(e, "INTERNAL_ERROR"))?;

    Ok(())
}

async fn remove_overlay(
    cdp: &crate::daemon::cdp_session::CdpSession,
    target_id: &str,
) -> Result<(), ActionResult> {
    let expression = format!(
        r#"(() => {{
            var el = document.getElementById({overlay_id});
            if (el) el.remove();
            return true;
        }})()"#,
        overlay_id = serde_json::to_string(OVERLAY_ID).unwrap_or_else(|_| "\"\"".to_string()),
    );

    cdp.execute_on_tab(
        target_id,
        "Runtime.evaluate",
        json!({
            "expression": expression,
            "returnByValue": true,
            "awaitPromise": false,
        }),
    )
    .await
    .map_err(|e| crate::daemon::cdp_session::cdp_error_to_result(e, "INTERNAL_ERROR"))?;

    Ok(())
}

async fn get_scroll_offsets(
    cdp: &crate::daemon::cdp_session::CdpSession,
    target_id: &str,
) -> Result<(f64, f64), ActionResult> {
    let resp = cdp
        .execute_on_tab(
            target_id,
            "Runtime.evaluate",
            json!({
                "expression": "({x: window.scrollX || 0, y: window.scrollY || 0})",
                "returnByValue": true,
                "awaitPromise": false,
            }),
        )
        .await
        .map_err(|e| crate::daemon::cdp_session::cdp_error_to_result(e, "INTERNAL_ERROR"))?;

    let value = &resp["result"]["result"]["value"];
    let x = value.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let y = value.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
    Ok((x, y))
}

/// Get clip JSON and Rect for a CSS selector.
async fn get_selector_rect(
    ctx: &mut TabContext,
    selector: &str,
) -> Result<(serde_json::Value, Rect), ActionResult> {
    let node_id = ctx.resolve_node(selector).await?;
    ctx.scroll_into_view(node_id).await?;

    // Resolve nodeId → objectId
    let resolve_resp = ctx
        .cdp
        .execute_on_tab(
            &ctx.target_id,
            "DOM.resolveNode",
            json!({ "nodeId": node_id }),
        )
        .await
        .map_err(|e| crate::daemon::cdp_session::cdp_error_to_result(e, "CDP_ERROR"))?;

    let object_id = resolve_resp
        .pointer("/result/object/objectId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ActionResult::fatal("CDP_ERROR", "failed to resolve selector to object"))?;

    let rect = get_rect_for_object(&ctx.cdp, &ctx.target_id, object_id, None)
        .await
        .ok_or_else(|| {
            ActionResult::fatal("CDP_ERROR", format!("failed to get rect for: {selector}"))
        })?;

    let clip = json!({
        "x": rect.x,
        "y": rect.y,
        "width": rect.width,
        "height": rect.height,
        "scale": 1,
    });

    Ok((clip, rect))
}

fn project_annotations(
    items: &[AnnotationItem],
    target_rect: Option<&Rect>,
    scroll: Option<(f64, f64)>,
) -> Vec<serde_json::Value> {
    items
        .iter()
        .map(|item| {
            let rect = if let Some(target) = target_rect {
                Rect {
                    x: item.rect.x - target.x,
                    y: item.rect.y - target.y,
                    width: item.rect.width,
                    height: item.rect.height,
                }
            } else if let Some((sx, sy)) = scroll {
                Rect {
                    x: item.rect.x + sx,
                    y: item.rect.y + sy,
                    width: item.rect.width,
                    height: item.rect.height,
                }
            } else {
                item.rect.clone()
            };

            json!({
                "ref": item.ref_id,
                "number": item.number,
                "role": item.role,
                "name": item.name,
                "box": {
                    "x": rect.x.round() as i64,
                    "y": rect.y.round() as i64,
                    "width": rect.width.round() as i64,
                    "height": rect.height.round() as i64,
                },
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_format_png() {
        assert_eq!(infer_format("/tmp/test.png"), "png");
    }

    #[test]
    fn test_infer_format_jpg() {
        assert_eq!(infer_format("/tmp/test.jpg"), "jpeg");
    }

    #[test]
    fn test_infer_format_jpeg() {
        assert_eq!(infer_format("/tmp/test.jpeg"), "jpeg");
    }

    #[test]
    fn test_infer_format_no_extension() {
        assert_eq!(infer_format("/tmp/screenshot"), "png");
    }

    #[test]
    fn test_infer_format_uppercase() {
        assert_eq!(infer_format("/tmp/test.JPG"), "jpeg");
        assert_eq!(infer_format("/tmp/test.PNG"), "png");
    }

    #[test]
    fn test_mime_type() {
        assert_eq!(mime_type("png"), "image/png");
        assert_eq!(mime_type("jpeg"), "image/jpeg");
    }

    #[test]
    fn test_filter_annotations_no_target() {
        let items = vec![
            AnnotationItem {
                ref_id: "e1".into(),
                number: 1,
                role: "button".into(),
                name: "OK".into(),
                rect: Rect {
                    x: 10.0,
                    y: 10.0,
                    width: 50.0,
                    height: 20.0,
                },
            },
            AnnotationItem {
                ref_id: "e2".into(),
                number: 2,
                role: "link".into(),
                name: "Home".into(),
                rect: Rect {
                    x: 200.0,
                    y: 200.0,
                    width: 40.0,
                    height: 20.0,
                },
            },
        ];
        let filtered = filter_annotations(items, None);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_annotations_with_target() {
        let items = vec![
            AnnotationItem {
                ref_id: "e1".into(),
                number: 1,
                role: "button".into(),
                name: "Inside".into(),
                rect: Rect {
                    x: 10.0,
                    y: 10.0,
                    width: 50.0,
                    height: 20.0,
                },
            },
            AnnotationItem {
                ref_id: "e2".into(),
                number: 2,
                role: "button".into(),
                name: "Outside".into(),
                rect: Rect {
                    x: 200.0,
                    y: 200.0,
                    width: 40.0,
                    height: 20.0,
                },
            },
        ];
        let target = Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        let filtered = filter_annotations(items, Some(&target));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].ref_id, "e1");
    }

    #[test]
    fn test_project_annotations_no_offset() {
        let items = vec![AnnotationItem {
            ref_id: "e1".into(),
            number: 1,
            role: "button".into(),
            name: "OK".into(),
            rect: Rect {
                x: 50.0,
                y: 100.0,
                width: 80.0,
                height: 30.0,
            },
        }];
        let projected = project_annotations(&items, None, None);
        assert_eq!(projected[0]["box"]["x"], 50);
        assert_eq!(projected[0]["box"]["y"], 100);
    }

    #[test]
    fn test_project_annotations_with_target() {
        let items = vec![AnnotationItem {
            ref_id: "e1".into(),
            number: 1,
            role: "button".into(),
            name: "OK".into(),
            rect: Rect {
                x: 25.0,
                y: 35.0,
                width: 40.0,
                height: 20.0,
            },
        }];
        let target = Rect {
            x: 10.0,
            y: 15.0,
            width: 100.0,
            height: 100.0,
        };
        let projected = project_annotations(&items, Some(&target), None);
        assert_eq!(projected[0]["box"]["x"], 15);
        assert_eq!(projected[0]["box"]["y"], 20);
    }

    #[test]
    fn test_project_annotations_with_scroll() {
        let items = vec![AnnotationItem {
            ref_id: "e1".into(),
            number: 1,
            role: "button".into(),
            name: "OK".into(),
            rect: Rect {
                x: 5.0,
                y: 12.0,
                width: 40.0,
                height: 20.0,
            },
        }];
        let projected = project_annotations(&items, None, Some((10.0, 1000.0)));
        assert_eq!(projected[0]["box"]["x"], 15);
        assert_eq!(projected[0]["box"]["y"], 1012);
    }
}

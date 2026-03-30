pub mod clear;
pub mod delete;
pub mod get;
pub mod list;
pub mod set;

use serde_json::Value;

/// Map a raw CDP cookie object to our canonical cookie shape.
pub fn map_cookie(c: &Value) -> Value {
    let expires_val = c
        .get("expires")
        .and_then(|v| v.as_f64())
        .and_then(|e| if e >= 0.0 { Some(Value::from(e)) } else { None })
        .unwrap_or(Value::Null);
    serde_json::json!({
        "name": c.get("name").and_then(|v| v.as_str()).unwrap_or(""),
        "value": c.get("value").and_then(|v| v.as_str()).unwrap_or(""),
        "domain": c.get("domain").and_then(|v| v.as_str()).unwrap_or(""),
        "path": c.get("path").and_then(|v| v.as_str()).unwrap_or("/"),
        "http_only": c.get("httpOnly").and_then(|v| v.as_bool()).unwrap_or(false),
        "secure": c.get("secure").and_then(|v| v.as_bool()).unwrap_or(false),
        "same_site": c.get("sameSite").and_then(|v| v.as_str()).unwrap_or(""),
        "expires": expires_val,
    })
}

/// Normalize a cookie domain for comparison by stripping a leading dot and lowercasing.
pub fn normalize_domain(d: &str) -> String {
    d.trim_start_matches('.').to_lowercase()
}

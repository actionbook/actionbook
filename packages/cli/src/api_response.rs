use serde_json::Value;

/// Unwrap the `data` field from the API response envelope `{ "success": true, "data": T }`.
/// Falls back to the original value if no `data` field is present.
pub fn unwrap_data(envelope: Value) -> Value {
    match envelope {
        Value::Object(mut map) => map.remove("data").unwrap_or(Value::Object(map)),
        other => other,
    }
}

/// Extract error message and available items from the API error envelope
/// `{ "success": false, "error": { "code": "...", "message": "...", ...extra } }`.
fn extract_api_error(body: &Value) -> (Option<String>, Option<Vec<String>>) {
    let error = match body.get("error") {
        Some(e) => e,
        None => return (None, None),
    };

    let message = error
        .get("message")
        .and_then(|v| v.as_str())
        .map(String::from);

    let available = error
        .get("available")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<_>>()
        })
        .filter(|v| !v.is_empty());

    (message, available)
}

/// Print error details from the API error envelope to stderr.
pub fn print_api_error(body: &Value) {
    let (message, available) = extract_api_error(body);
    if let Some(msg) = message {
        eprintln!("{msg}");
    }
    if let Some(items) = available {
        eprintln!("Available: {}", items.join(", "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unwrap_data_extracts_data_field() {
        let envelope = json!({ "success": true, "data": [1, 2, 3] });
        assert_eq!(unwrap_data(envelope), json!([1, 2, 3]));
    }

    #[test]
    fn unwrap_data_extracts_object_data() {
        let envelope = json!({ "success": true, "data": { "name": "test" } });
        assert_eq!(unwrap_data(envelope), json!({ "name": "test" }));
    }

    #[test]
    fn unwrap_data_falls_back_without_data_field() {
        let value = json!({ "name": "test" });
        assert_eq!(unwrap_data(value.clone()), value);
    }

    #[test]
    fn unwrap_data_handles_non_object() {
        assert_eq!(unwrap_data(json!("hello")), json!("hello"));
        assert_eq!(unwrap_data(json!(42)), json!(42));
    }

    #[test]
    fn unwrap_data_null_data_field() {
        let envelope = json!({ "success": true, "data": null });
        assert_eq!(unwrap_data(envelope), json!(null));
    }

    #[test]
    fn extract_error_message() {
        let body = json!({
            "success": false,
            "error": { "code": "NOT_FOUND", "message": "Site \"foo\" not found." }
        });
        let (message, available) = extract_api_error(&body);
        assert_eq!(message.as_deref(), Some("Site \"foo\" not found."));
        assert!(available.is_none());
    }

    #[test]
    fn extract_error_with_available() {
        let body = json!({
            "success": false,
            "error": {
                "code": "NOT_FOUND",
                "message": "Resource \"x\" not found.",
                "available": ["pages", "blocks", "databases"]
            }
        });
        let (message, available) = extract_api_error(&body);
        assert_eq!(message.as_deref(), Some("Resource \"x\" not found."));
        assert_eq!(
            available.as_deref(),
            Some(
                vec![
                    "pages".to_string(),
                    "blocks".to_string(),
                    "databases".to_string()
                ]
                .as_slice()
            )
        );
    }

    #[test]
    fn extract_error_no_error_field() {
        let body = json!({ "something": "else" });
        let (message, available) = extract_api_error(&body);
        assert!(message.is_none());
        assert!(available.is_none());
    }

    #[test]
    fn extract_error_empty_available_is_none() {
        let body = json!({
            "success": false,
            "error": { "code": "BAD_REQUEST", "message": "Missing param", "available": [] }
        });
        let (_, available) = extract_api_error(&body);
        assert!(available.is_none());
    }
}

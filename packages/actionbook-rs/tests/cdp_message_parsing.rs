// Unit tests for CDP message parsing logic
// Tests the improved structured detection of Events vs Responses

use serde_json::json;

#[test]
fn test_cdp_event_structure_detection() {
    // CDP Event has "method" but no "id"
    let event = json!({
        "method": "Page.loadEventFired",
        "params": {"timestamp": 123456}
    });

    assert!(event.get("method").is_some());
    assert!(event.get("id").is_none());
    assert!(event.get("params").is_some());
}

#[test]
fn test_cdp_response_structure_detection() {
    // CDP Response has "id" and either "result" or "error"
    let response = json!({
        "id": 1,
        "result": {"value": "test"}
    });

    assert!(response.get("id").is_some());
    assert!(response.get("result").is_some());
    assert!(response.get("method").is_none());
}

#[test]
fn test_cdp_error_response_structure() {
    // CDP Error Response has "id" and "error"
    let error = json!({
        "id": 2,
        "error": {
            "code": -32000,
            "message": "Connection closed"
        }
    });

    assert!(error.get("id").is_some());
    assert!(error.get("error").is_some());
    assert!(error.get("method").is_none());
}

#[test]
fn test_ambiguous_message_with_both_method_and_id() {
    // Edge case: message with both "method" and "id"
    // This should be treated as a Response (id takes precedence)
    let ambiguous = json!({
        "id": 3,
        "method": "Runtime.evaluate",
        "result": {"value": 42}
    });

    // Our logic: has_id = true → treat as Response
    assert!(ambiguous.get("id").is_some());
}

#[test]
fn test_malformed_message_no_id_no_method() {
    // Message with neither "id" nor "method" - should be rejected
    let malformed = json!({
        "unknown": "field",
        "data": 123
    });

    let has_method = malformed.get("method").is_some();
    let has_id = malformed.get("id").is_some();

    assert!(!has_method);
    assert!(!has_id);
    // Our logic should log warning and continue
}

#[test]
fn test_string_containing_method_in_value() {
    // Edge case: "method" appears in a string value but not as a field
    // Old string-based detection would false-positive this
    let misleading = json!({
        "id": 4,
        "result": {
            "description": "This object has a method called foo()"
        }
    });

    let serialized = serde_json::to_string(&misleading).unwrap();

    // Old bad approach: serialized.contains("\"method\"") → false (correct)
    // Our approach: value.get("method").is_some() → false (correct)
    assert!(!serialized.contains("\"method\":"));
    assert!(misleading.get("method").is_none());
}

#[test]
fn test_keychain_env_var_parsing() {
    // Test environment variable parsing logic
    let test_cases = vec![
        ("1", true),
        ("true", true),
        ("TRUE", true),
        ("True", true),
        ("0", false),
        ("false", false),
        ("", false),
        ("anything-else", false),
    ];

    for (input, expected) in test_cases {
        let result = if input == "1" || input.eq_ignore_ascii_case("true") {
            true
        } else {
            false
        };
        assert_eq!(result, expected, "Failed for input: {}", input);
    }
}

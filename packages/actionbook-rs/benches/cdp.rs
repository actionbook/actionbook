// CDP message parsing benchmarks
//
// Tests CDP protocol message parsing performance with typed vs dynamic deserialization.
// Key metrics: parse time for responses, events, and errors.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// Sample CDP messages
const CDP_RESPONSE: &str = r#"{"id":1,"result":{"value":"test result"}}"#;
const CDP_EVENT: &str = r#"{"method":"Page.loadEventFired","params":{"timestamp":123456}}"#;
const CDP_ERROR: &str = r#"{"id":2,"error":{"code":-32000,"message":"Connection closed"}}"#;

// Current pattern: dynamic Value access
fn bench_cdp_parse_value(c: &mut Criterion) {
    c.bench_function("cdp_parse_response_value", |b| {
        b.iter(|| {
            let response: Value = serde_json::from_str(black_box(CDP_RESPONSE)).unwrap();
            let _id = response.get("id").and_then(|v| v.as_i64());
            let _result = response.get("result");
        });
    });

    c.bench_function("cdp_parse_event_value", |b| {
        b.iter(|| {
            let event: Value = serde_json::from_str(black_box(CDP_EVENT)).unwrap();
            let _method = event.get("method").and_then(|v| v.as_str());
            let _params = event.get("params");
        });
    });
}

// Proposed pattern: typed envelope
#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum CdpMessage {
    Response {
        id: i64,
        #[serde(default)]
        result: Option<Value>,
        #[serde(default)]
        error: Option<CdpError>,
    },
    Event {
        method: String,
        params: Value,
    },
}

#[derive(Deserialize, Debug)]
struct CdpError {
    code: i64,
    message: String,
    #[serde(default)]
    data: Option<Value>,
}

fn bench_cdp_parse_typed(c: &mut Criterion) {
    c.bench_function("cdp_parse_response_typed", |b| {
        b.iter(|| {
            let message: CdpMessage = serde_json::from_str(black_box(CDP_RESPONSE)).unwrap();
            match message {
                CdpMessage::Response { id, result, .. } => {
                    let _id = id;
                    let _result = result;
                }
                _ => panic!("Expected response"),
            }
        });
    });

    c.bench_function("cdp_parse_event_typed", |b| {
        b.iter(|| {
            let message: CdpMessage = serde_json::from_str(black_box(CDP_EVENT)).unwrap();
            match message {
                CdpMessage::Event { method, params } => {
                    let _method = method;
                    let _params = params;
                }
                _ => panic!("Expected event"),
            }
        });
    });

    c.bench_function("cdp_parse_error_typed", |b| {
        b.iter(|| {
            let message: CdpMessage = serde_json::from_str(black_box(CDP_ERROR)).unwrap();
            match message {
                CdpMessage::Response { error, .. } => {
                    let _error = error;
                }
                _ => panic!("Expected error response"),
            }
        });
    });
}

criterion_group!(cdp_benches, bench_cdp_parse_value, bench_cdp_parse_typed);
criterion_main!(cdp_benches);

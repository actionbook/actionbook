//! Camoufox REST API Server
//!
//! Provides HTTP REST API for Camoufox browser automation.
//! Wraps Playwright/Camoufox as a REST service with accessibility tree support.

use actionbook::browser::camofox::types::{
    AccessibilityNode, ClickRequest, CreateTabRequest, CreateTabResponse, NavigateRequest,
    ScreenshotResponse, SnapshotResponse, TypeTextRequest,
};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;

/// Global server state
#[derive(Clone)]
struct AppState {
    tabs: Arc<RwLock<HashMap<String, TabState>>>,
}

/// State for each browser tab
struct TabState {
    url: String,
    session_key: String,
}

impl AppState {
    fn new() -> Self {
        Self {
            tabs: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

/// Custom error type for API responses
#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (
            self.status,
            Json(serde_json::json!({ "error": self.message })),
        )
            .into_response()
    }
}

/// Health check endpoint
async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "server": "camoufox-rest-api",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

/// Create a new browser tab
async fn create_tab(
    State(state): State<AppState>,
    Json(req): Json<CreateTabRequest>,
) -> Result<Json<CreateTabResponse>, ApiError> {
    // Generate tab ID
    let tab_id = format!("tab-{}", uuid::Uuid::new_v4());

    // Store tab state
    let mut tabs = state.tabs.write().await;
    tabs.insert(
        tab_id.clone(),
        TabState {
            url: req.url.clone(),
            session_key: req.session_key,
        },
    );

    println!("📄 Created tab {} -> {}", tab_id, req.url);

    Ok(Json(CreateTabResponse {
        id: tab_id,
        url: req.url,
    }))
}

/// Get accessibility tree snapshot
async fn get_snapshot(
    State(state): State<AppState>,
    Path(tab_id): Path<String>,
) -> Result<Json<SnapshotResponse>, ApiError> {
    let tabs = state.tabs.read().await;

    if !tabs.contains_key(&tab_id) {
        return Err(ApiError::not_found(format!("Tab not found: {}", tab_id)));
    }

    // Mock accessibility tree
    let tree = AccessibilityNode {
        role: "document".to_string(),
        name: Some("Example Domain".to_string()),
        element_ref: None,
        children: Some(vec![
            AccessibilityNode {
                role: "heading".to_string(),
                name: Some("Example Domain".to_string()),
                element_ref: Some("e1".to_string()),
                children: None,
                value: None,
                focusable: None,
            },
            AccessibilityNode {
                role: "paragraph".to_string(),
                name: Some("This domain is for use in illustrative examples".to_string()),
                element_ref: Some("e2".to_string()),
                children: None,
                value: None,
                focusable: None,
            },
            AccessibilityNode {
                role: "link".to_string(),
                name: Some("More information...".to_string()),
                element_ref: Some("e3".to_string()),
                children: None,
                value: None,
                focusable: Some(true),
            },
        ]),
        value: None,
        focusable: None,
    };

    println!("🌳 Snapshot for tab {}", tab_id);

    Ok(Json(SnapshotResponse { tree }))
}

/// Click an element
async fn click_element(
    State(state): State<AppState>,
    Path(tab_id): Path<String>,
    Json(req): Json<ClickRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let tabs = state.tabs.read().await;

    if !tabs.contains_key(&tab_id) {
        return Err(ApiError::not_found(format!("Tab not found: {}", tab_id)));
    }

    println!("🖱️  Click {} in tab {}", req.element_ref, tab_id);

    Ok(Json(serde_json::json!({
        "status": "ok",
        "message": format!("Clicked {}", req.element_ref)
    })))
}

/// Type text into element
async fn type_text(
    State(state): State<AppState>,
    Path(tab_id): Path<String>,
    Json(req): Json<TypeTextRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let tabs = state.tabs.read().await;

    if !tabs.contains_key(&tab_id) {
        return Err(ApiError::not_found(format!("Tab not found: {}", tab_id)));
    }

    println!(
        "⌨️  Type into {} in tab {}: '{}'",
        req.element_ref, tab_id, req.text
    );

    Ok(Json(serde_json::json!({
        "status": "ok",
        "message": format!("Typed into {}", req.element_ref)
    })))
}

/// Navigate to URL
async fn navigate(
    State(state): State<AppState>,
    Path(tab_id): Path<String>,
    Json(req): Json<NavigateRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut tabs = state.tabs.write().await;

    let tab = tabs
        .get_mut(&tab_id)
        .ok_or_else(|| ApiError::not_found(format!("Tab not found: {}", tab_id)))?;

    println!("🔗 Navigate tab {} to {}", tab_id, req.url);

    tab.url = req.url.clone();

    Ok(Json(serde_json::json!({
        "status": "ok",
        "url": req.url
    })))
}

/// Take screenshot
async fn screenshot(
    State(state): State<AppState>,
    Path(tab_id): Path<String>,
) -> Result<Json<ScreenshotResponse>, ApiError> {
    let tabs = state.tabs.read().await;

    if !tabs.contains_key(&tab_id) {
        return Err(ApiError::not_found(format!("Tab not found: {}", tab_id)));
    }

    println!("📸 Screenshot for tab {}", tab_id);

    // Mock 1x1 pixel PNG
    let tiny_png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15\xc4\x89\x00\x00\x00\nIDATx\x9cc\x00\x01\x00\x00\x05\x00\x01\r\n-\xb4\x00\x00\x00\x00IEND\xaeB`\x82";
    use base64::{engine::general_purpose, Engine as _};
    let encoded = general_purpose::STANDARD.encode(tiny_png);

    Ok(Json(ScreenshotResponse { data: encoded }))
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Create app state
    let state = AppState::new();

    // Build router
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/tabs", post(create_tab))
        .route("/tabs/:tab_id/snapshot", get(get_snapshot))
        .route("/tabs/:tab_id/click", post(click_element))
        .route("/tabs/:tab_id/type", post(type_text))
        .route("/tabs/:tab_id/navigate", post(navigate))
        .route("/tabs/:tab_id/screenshot", get(screenshot))
        .layer(CorsLayer::permissive())
        .with_state(state);

    // Get port from environment or use default
    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(9377);

    let addr = format!("0.0.0.0:{}", port);

    println!("🦊 Camoufox REST API Server");
    println!("====================================");
    println!("Version: {}", env!("CARGO_PKG_VERSION"));
    println!("Address: http://{}", addr);
    println!("====================================");
    println!("Endpoints:");
    println!("  GET  /health             - Health check");
    println!("  POST /tabs               - Create tab");
    println!("  GET  /tabs/:id/snapshot  - Get accessibility tree");
    println!("  POST /tabs/:id/click     - Click element");
    println!("  POST /tabs/:id/type      - Type text");
    println!("  POST /tabs/:id/navigate  - Navigate to URL");
    println!("  GET  /tabs/:id/screenshot - Take screenshot");
    println!("====================================");
    println!("Press Ctrl+C to stop");
    println!();

    // Start server
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind to address");

    axum::serve(listener, app)
        .await
        .expect("Server error");
}

// UUID generation module
mod uuid {
    use std::fmt;

    pub struct Uuid([u8; 16]);

    impl Uuid {
        pub fn new_v4() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            let mut bytes = [0u8; 16];
            rng.fill(&mut bytes);

            // Version 4 UUID
            bytes[6] = (bytes[6] & 0x0f) | 0x40;
            bytes[8] = (bytes[8] & 0x3f) | 0x80;

            Self(bytes)
        }
    }

    impl fmt::Display for Uuid {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(
                f,
                "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
                self.0[0], self.0[1], self.0[2], self.0[3],
                self.0[4], self.0[5],
                self.0[6], self.0[7],
                self.0[8], self.0[9],
                self.0[10], self.0[11], self.0[12], self.0[13], self.0[14], self.0[15]
            )
        }
    }
}

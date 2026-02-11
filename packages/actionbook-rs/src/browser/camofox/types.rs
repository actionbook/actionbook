//! Type definitions for Camoufox REST API

use serde::{Deserialize, Serialize};

/// Request to create a new browser tab
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTabRequest {
    pub user_id: String,
    pub session_key: String,
    pub url: String,
}

/// Response from creating a new tab
#[derive(Debug, Clone, Deserialize)]
pub struct CreateTabResponse {
    pub id: String,
    pub url: String,
}

/// Request to click an element
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClickRequest {
    pub user_id: String,
    pub element_ref: String, // "e1", "e2", etc.
}

/// Request to type text into an element
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeTextRequest {
    pub user_id: String,
    pub element_ref: String,
    pub text: String,
}

/// Request to navigate to a URL
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NavigateRequest {
    pub user_id: String,
    pub url: String,
}

/// Accessibility tree node representing a UI element
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessibilityNode {
    /// ARIA role (button, textbox, link, etc.)
    pub role: String,

    /// Element name/label (text content, aria-label, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Stable element reference (e1, e2, e3, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element_ref: Option<String>,

    /// Child nodes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<AccessibilityNode>>,

    /// Additional attributes for matching
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,

    /// Whether the element is focusable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focusable: Option<bool>,
}

/// Response from getting accessibility tree snapshot
#[derive(Debug, Clone, Deserialize)]
pub struct SnapshotResponse {
    pub tree: AccessibilityNode,
}

/// Response from screenshot endpoint
#[derive(Debug, Clone, Deserialize)]
pub struct ScreenshotResponse {
    /// Base64-encoded PNG image
    pub data: String,
}

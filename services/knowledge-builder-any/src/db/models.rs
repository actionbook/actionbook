//! Database models matching the Drizzle ORM schema

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

// ============================================================================
// Build Tasks
// ============================================================================

/// BuildTask - Matches build_tasks table
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct BuildTask {
    pub id: i32,
    pub source_id: Option<i32>,
    pub source_url: String,
    pub source_name: Option<String>,
    pub source_category: String,
    pub stage: String,
    pub stage_status: String,
    pub config: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub knowledge_started_at: Option<DateTime<Utc>>,
    pub knowledge_completed_at: Option<DateTime<Utc>>,
    pub action_started_at: Option<DateTime<Utc>>,
    pub action_completed_at: Option<DateTime<Utc>>,
}

/// Source category types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceCategory {
    Help,
    Unknown,
    Any,
}

impl SourceCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceCategory::Help => "help",
            SourceCategory::Unknown => "unknown",
            SourceCategory::Any => "any",
        }
    }
}

/// Build task stage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildTaskStage {
    Init,
    KnowledgeBuild,
    ActionBuild,
    Completed,
    Error,
}

impl BuildTaskStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            BuildTaskStage::Init => "init",
            BuildTaskStage::KnowledgeBuild => "knowledge_build",
            BuildTaskStage::ActionBuild => "action_build",
            BuildTaskStage::Completed => "completed",
            BuildTaskStage::Error => "error",
        }
    }
}

/// Stage execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StageStatus {
    Pending,
    Running,
    Completed,
    Error,
}

impl StageStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            StageStatus::Pending => "pending",
            StageStatus::Running => "running",
            StageStatus::Completed => "completed",
            StageStatus::Error => "error",
        }
    }
}

// ============================================================================
// Sources
// ============================================================================

/// Source - Matches sources table
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Source {
    pub id: i32,
    pub name: String,
    pub base_url: String,
    pub description: Option<String>,
    pub crawl_config: serde_json::Value,
    pub domain: Option<String>,
    pub tags: Option<serde_json::Value>,
    pub health_score: Option<i32>,
    pub last_crawled_at: Option<DateTime<Utc>>,
    pub last_recorded_at: Option<DateTime<Utc>>,
    pub current_version_id: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// NewSource - For inserting new sources
#[derive(Debug, Clone, Serialize)]
pub struct NewSource {
    pub name: String,
    pub base_url: String,
    pub description: Option<String>,
    pub domain: Option<String>,
}

// ============================================================================
// Source Versions
// ============================================================================

/// SourceVersion status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceVersionStatus {
    Building,
    Active,
    Archived,
}

impl SourceVersionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceVersionStatus::Building => "building",
            SourceVersionStatus::Active => "active",
            SourceVersionStatus::Archived => "archived",
        }
    }
}

/// SourceVersion - Matches source_versions table
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct SourceVersion {
    pub id: i32,
    pub source_id: i32,
    pub version_number: i32,
    pub status: String,
    pub commit_message: Option<String>,
    pub created_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
}

/// NewSourceVersion - For inserting new source versions
#[derive(Debug, Clone)]
pub struct NewSourceVersion {
    pub source_id: i32,
    pub commit_message: Option<String>,
    pub created_by: Option<String>,
}

// ============================================================================
// Documents
// ============================================================================

/// Document - Matches documents table
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Document {
    pub id: i32,
    pub source_id: i32,
    pub source_version_id: Option<i32>,
    pub url: String,
    pub url_hash: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub content_text: Option<String>,
    pub content_html: Option<String>,
    pub content_md: Option<String>,
    pub parent_id: Option<i32>,
    pub depth: i32,
    pub breadcrumb: serde_json::Value,
    pub word_count: Option<i32>,
    pub language: Option<String>,
    pub content_hash: Option<String>,
    pub elements: Option<String>,
    pub status: String,
    pub version: i32,
    pub published_at: Option<DateTime<Utc>>,
    pub crawled_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// NewDocument - For inserting new documents
#[derive(Debug, Clone, Serialize)]
pub struct NewDocument {
    pub source_id: i32,
    pub source_version_id: Option<i32>,
    pub url: String,
    pub url_hash: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub content_md: Option<String>,
    pub content_hash: Option<String>,
    pub depth: i32,
}

// ============================================================================
// Chunks
// ============================================================================

/// Chunk - Matches chunks table (without embedding field for query results)
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Chunk {
    pub id: i32,
    pub document_id: i32,
    pub source_version_id: Option<i32>,
    pub content: String,
    pub content_hash: String,
    pub chunk_index: i32,
    pub start_char: i32,
    pub end_char: i32,
    pub heading: Option<String>,
    pub heading_hierarchy: serde_json::Value,
    pub token_count: i32,
    pub embedding_model: Option<String>,
    pub elements: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// NewChunk - For inserting new chunks
#[derive(Debug, Clone)]
pub struct NewChunk {
    pub document_id: i32,
    pub source_version_id: Option<i32>,
    pub content: String,
    pub content_hash: String,
    pub chunk_index: i32,
    pub start_char: i32,
    pub end_char: i32,
    pub heading: Option<String>,
    pub heading_hierarchy: Vec<HeadingItem>,
    pub token_count: i32,
    pub embedding: Option<Vec<f32>>,
    pub embedding_model: Option<String>,
}

/// HeadingItem - For heading hierarchy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadingItem {
    pub level: i32,
    pub text: String,
}

//! Error types for handbook-builder

use thiserror::Error;

#[derive(Error, Debug)]
pub enum HandbookError {
    #[error("Failed to fetch URL: {url}")]
    FetchError {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("HTTP error {status} for URL: {url}")]
    HttpStatusError { url: String, status: u16 },

    #[error("Failed to fetch URL after {attempts} attempts: {url} (last error: {last_error})")]
    RetryExhausted {
        url: String,
        attempts: u32,
        last_error: String,
    },

    #[error("Failed to parse HTML: {0}")]
    ParseError(String),

    #[error("Claude API error: {0}")]
    ClaudeError(String),

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("IO error: {0}")]
    IoError(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Prompt file not found for site: {0}")]
    PromptNotFound(String),

    #[error("File system error")]
    FsError(#[from] std::io::Error),

    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Embedding error: {0}")]
    EmbeddingError(String),

    #[error("OpenAI API error: {0}")]
    OpenAiError(#[from] async_openai::error::OpenAIError),

    #[error("URL parse error: {0}")]
    UrlParseError(#[from] url::ParseError),

    #[error("Task timeout")]
    TaskTimeout,
}

pub type Result<T> = std::result::Result<T, HandbookError>;

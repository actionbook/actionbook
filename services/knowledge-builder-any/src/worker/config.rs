//! Worker configuration

use crate::embedding::DEFAULT_EMBEDDING_MODEL;
use std::time::Duration;

/// Worker configuration
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// Poll interval when no tasks available
    pub poll_interval: Duration,

    /// Batch size for task fetching (currently always 1)
    pub batch_size: usize,

    /// Task timeout
    pub task_timeout: Duration,

    /// Enable embedding generation
    pub enable_embeddings: bool,

    /// Embedding model to use
    pub embedding_model: String,

    /// Embedding dimensions
    pub embedding_dimensions: usize,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(5),
            batch_size: 1,
            task_timeout: Duration::from_secs(300), // 5 minutes
            enable_embeddings: true,
            embedding_model: DEFAULT_EMBEDDING_MODEL.to_string(),
            embedding_dimensions: 1536,
        }
    }
}

impl WorkerConfig {
    /// Create a new config builder
    pub fn builder() -> WorkerConfigBuilder {
        WorkerConfigBuilder::default()
    }
}

/// Builder for WorkerConfig
pub struct WorkerConfigBuilder {
    config: WorkerConfig,
}

impl WorkerConfigBuilder {
    /// Set poll interval
    pub fn poll_interval(mut self, duration: Duration) -> Self {
        self.config.poll_interval = duration;
        self
    }

    /// Set poll interval in seconds
    pub fn poll_interval_secs(mut self, secs: u64) -> Self {
        self.config.poll_interval = Duration::from_secs(secs);
        self
    }

    /// Set task timeout
    pub fn task_timeout(mut self, duration: Duration) -> Self {
        self.config.task_timeout = duration;
        self
    }

    /// Enable/disable embeddings
    pub fn enable_embeddings(mut self, enable: bool) -> Self {
        self.config.enable_embeddings = enable;
        self
    }

    /// Set embedding model
    pub fn embedding_model(mut self, model: &str) -> Self {
        self.config.embedding_model = model.to_string();
        self
    }

    /// Build the config
    pub fn build(self) -> WorkerConfig {
        self.config
    }
}

impl Default for WorkerConfigBuilder {
    fn default() -> Self {
        Self {
            config: WorkerConfig::default(),
        }
    }
}

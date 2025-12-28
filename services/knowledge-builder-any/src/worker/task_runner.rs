//! Task runner - main worker loop

use crate::db::{build_tasks, DbPool};
use crate::error::{HandbookError, Result};
use crate::worker::{TaskProcessor, WorkerConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::{error, info};

/// Task runner that polls and processes build_tasks
pub struct TaskRunner {
    pool: DbPool,
    config: WorkerConfig,
    processor: TaskProcessor,
    shutdown: Arc<AtomicBool>,
}

impl TaskRunner {
    /// Create a new task runner
    pub fn new(pool: DbPool, config: WorkerConfig, processor: TaskProcessor) -> Self {
        Self {
            pool,
            config,
            processor,
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Get a handle to signal shutdown
    pub fn shutdown_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.shutdown)
    }

    /// Main worker loop
    ///
    /// Polls for pending tasks and processes them until shutdown is signaled
    pub async fn run(&self) -> Result<()> {
        info!("Starting knowledge-builder worker...");
        info!("Poll interval: {:?}", self.config.poll_interval);
        info!("Task timeout: {:?}", self.config.task_timeout);
        info!(
            "Embeddings: {}",
            if self.config.enable_embeddings {
                "enabled"
            } else {
                "disabled"
            }
        );

        loop {
            // Check for shutdown signal
            if self.shutdown.load(Ordering::Relaxed) {
                info!("Shutdown signal received, stopping worker...");
                break;
            }

            match self.process_one_task().await {
                Ok(true) => {
                    // Task processed, continue immediately
                    info!("Task completed, checking for next task...");
                }
                Ok(false) => {
                    // No tasks available, wait before polling
                    info!(
                        "No pending tasks, sleeping for {:?}",
                        self.config.poll_interval
                    );
                    sleep(self.config.poll_interval).await;
                }
                Err(e) => {
                    error!("Worker error: {}", e);
                    // Wait a bit before retrying after error
                    sleep(Duration::from_secs(10)).await;
                }
            }
        }

        info!("Worker stopped");
        Ok(())
    }

    /// Process a single task (useful for testing with --once flag)
    ///
    /// Returns:
    /// - Ok(true) if a task was processed
    /// - Ok(false) if no tasks were available
    /// - Err on error
    pub async fn process_one_task(&self) -> Result<bool> {
        // Atomically claim a pending task
        let task = match build_tasks::claim_next_pending_task(&self.pool).await? {
            Some(t) => t,
            None => return Ok(false),
        };

        let task_id = task.id;
        let source_url = task.source_url.clone();
        info!("Claimed task {}: {}", task_id, source_url);

        // Process with timeout
        let result = tokio::time::timeout(
            self.config.task_timeout,
            self.processor.process(&self.pool, &task),
        )
        .await;

        match result {
            Ok(Ok(source_id)) => {
                info!(
                    "Task {} completed successfully, source_id={}",
                    task_id, source_id
                );
                build_tasks::complete_task(&self.pool, task_id, source_id).await?;
            }
            Ok(Err(e)) => {
                error!("Task {} failed: {}", task_id, e);
                build_tasks::error_task(&self.pool, task_id, &e.to_string()).await?;
            }
            Err(_) => {
                error!("Task {} timed out after {:?}", task_id, self.config.task_timeout);
                build_tasks::error_task(&self.pool, task_id, "Task timeout").await?;
                return Err(HandbookError::TaskTimeout);
            }
        }

        Ok(true)
    }

    /// Run once and exit (for testing)
    pub async fn run_once(&self) -> Result<bool> {
        info!("Running worker in single-task mode...");
        self.process_one_task().await
    }
}

/// Setup signal handlers for graceful shutdown
pub fn setup_signal_handler(shutdown: Arc<AtomicBool>) {
    tokio::spawn(async move {
        match tokio::signal::ctrl_c().await {
            Ok(()) => {
                info!("Received Ctrl+C, initiating shutdown...");
                shutdown.store(true, Ordering::Relaxed);
            }
            Err(e) => {
                error!("Failed to listen for Ctrl+C: {}", e);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    // Integration tests require database - see tests/ directory
}

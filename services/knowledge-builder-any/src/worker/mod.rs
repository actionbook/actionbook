//! Worker module for polling and processing build_tasks
//!
//! This module provides:
//! - TaskRunner: Main worker loop that polls for pending tasks
//! - TaskProcessor: Processes individual tasks (handbook generation + storage)
//! - WorkerConfig: Configuration for the worker

pub mod config;
pub mod processor;
pub mod task_runner;

pub use config::WorkerConfig;
pub use processor::TaskProcessor;
pub use task_runner::{setup_signal_handler, TaskRunner};

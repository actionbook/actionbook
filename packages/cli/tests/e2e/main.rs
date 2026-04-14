//! E2E test suite for actionbook CLI v1.0.0.
//!
//! Single binary entry point — all test modules compile into one binary
//! to avoid per-file link overhead.
//!
//! Tests are gated by `RUN_E2E_TESTS=true`. Without this env var every
//! test is skipped (returns immediately).
//!
//! Run with:
//!   RUN_E2E_TESTS=true cargo test --test e2e -- --test-threads=1 --nocapture

mod batch_snapshot;
mod bridge;
mod browser_lifecycle;
mod cloud_mode;
mod cookies;
mod describe_state;
mod element_details;
mod element_read;
mod extension;
mod harness;
mod iframe;
mod inspect_point;
mod interaction;
mod logs;
mod navigation;
mod network;
mod page_info;
mod pdf;
mod query;
mod screenshot;
mod snapshot;
mod storage;
mod tab_management;
mod timeout;
mod wait;
mod windows_daemon;

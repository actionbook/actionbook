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

mod browser_lifecycle;
mod cloud_mode;
mod element_details;
mod element_read;
mod harness;
mod inspect_point;
mod interaction;
mod navigation;
mod page_info;
mod snapshot;
mod tab_management;

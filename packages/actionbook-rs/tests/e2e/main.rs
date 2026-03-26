//! E2E test suite for actionbook CLI.
//!
//! Single binary entry point — all test modules are compiled into one binary to
//! avoid the per-file link overhead that Rust's default `tests/` layout incurs.
//!
//! Tests are gated by `RUN_E2E_TESTS=true`. Without this env var every test is
//! skipped (returns immediately).
//!
//! Run with:
//!   RUN_E2E_TESTS=true cargo test --test e2e -- --test-threads=1 --nocapture

mod browser_basic;
mod browser_data;
mod browser_errors;
mod browser_interaction;
mod browser_lifecycle;
mod browser_navigation;
mod browser_observation;
mod browser_tab;
mod browser_waiting;
mod harness;

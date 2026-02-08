// Library re-exports for integration testing.
// The main binary is in main.rs; this exposes selected modules for tests.

pub mod browser;
pub mod cli;
pub mod config;
pub mod error;

mod api;
pub mod commands;

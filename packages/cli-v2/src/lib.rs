pub const BUILD_VERSION: &str = env!("BUILD_VERSION");

pub mod action;
pub mod action_result;
pub mod browser;
pub mod cli;
pub mod config;
pub mod daemon;
pub mod error;
pub mod output;
pub mod setup;
pub mod types;
pub mod utils;

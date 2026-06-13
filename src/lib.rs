//! # TaskMaster Library
//!
//! A reward management server that monitors social media interactions and
//! integrates with the Quantus Network. This library provides the core
//! functionality for managing rewards and handling HTTP API requests.

pub mod args;
pub mod config;
pub mod db_persistence;
pub mod errors;
pub mod handlers;
pub mod http_server;
pub mod metrics;
pub mod middlewares;
pub mod models;
pub mod repositories;
pub mod routes;
pub mod services;
pub mod utils;

// Re-export commonly used types
pub use config::Config;
pub use db_persistence::DbError;
pub use errors::{AppError, AppResult};
pub use http_server::AppState;

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Library name
pub const NAME: &str = env!("CARGO_PKG_NAME");

/// Get the library version
pub fn version() -> &'static str {
    VERSION
}

/// Get the library name
pub fn name() -> &'static str {
    NAME
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!version().is_empty());
    }

    #[test]
    fn test_name() {
        assert_eq!(name(), "task-master");
    }
}

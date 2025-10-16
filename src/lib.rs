//! # TaskMaster Library
//!
//! A task management server that creates reversible blockchain transactions
//! using the Quantus Network. This library provides the core functionality
//! for managing tasks, interacting with the blockchain, and handling
//! HTTP API requests.

pub mod config;
pub mod repositories;
pub mod errors;
pub mod models;
pub mod db_persistence;
pub mod services;
pub mod http_server;
pub mod utils;
pub mod routes;
pub mod handlers;
pub mod middlewares;

// Re-export commonly used types
pub use errors::{AppError, AppResult};
pub use config::Config;
pub use services::graphql_client::{GraphqlClient, SyncStats, Transfer};
pub use http_server::{AppState};
pub use services::reverser::{ReversalStats, ReverserService};
pub use services::task_generator::TaskGenerator;
pub use services::transaction_manager::TransactionManager;

// Re-export errors
pub use db_persistence::DbError;
pub use services::graphql_client::GraphqlError;
pub use services::reverser::ReverserError;
pub use services::ethereum_service::EthAddressAssociation;
pub use services::task_generator::TaskGeneratorError;
pub use services::transaction_manager::TransactionError;

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

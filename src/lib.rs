//! # TaskMaster Library
//!
//! A task management server that creates reversible blockchain transactions
//! using the Quantus Network. This library provides the core functionality
//! for managing tasks, interacting with the blockchain, and handling
//! HTTP API requests.

pub mod config;
pub mod db_persistence;
pub mod graphql_client;
pub mod http_server;
pub mod reverser;
pub mod signature_verification;
pub mod task_generator;
pub mod transaction_manager;

// Re-export commonly used types
pub use config::Config;
pub use graphql_client::{GraphqlClient, SyncStats, Transfer};
pub use http_server::{AppState, CompleteTaskRequest, CompleteTaskResponse};
pub use reverser::{ReversalStats, ReverserService};
pub use task_generator::TaskGenerator;
pub use transaction_manager::TransactionManager;

// Re-export errors
pub use graphql_client::GraphqlError;
pub use http_server::HttpServerError;
pub use reverser::ReverserError;
pub use signature_verification::{EthAddressAssociation, SignatureError};
pub use task_generator::TaskGeneratorError;
pub use transaction_manager::TransactionError;

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

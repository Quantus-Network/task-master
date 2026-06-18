//! # TaskMaster Library
//!
//! A task management server that monitors social media interactions and
//! integrates with the Quantus Network. This library provides the core
//! functionality for handling HTTP API requests.

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

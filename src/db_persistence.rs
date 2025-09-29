// src/repositories/db_persistence.rs

use sqlx::{postgres::PgPoolOptions, PgPool};

// Import your repositories
use crate::repositories::{
    address::AddressRepository, referral::ReferralRepository, task::TaskRepository,
};
use crate::repositories::DbResult; // Keep your DbResult type

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("Task not found: {0}")]
    TaskNotFound(String),
    #[error("Address not found: {0}")]
    AddressNotFound(String),
    #[error("Invalid task status: {0}")]
    InvalidStatus(String),
}

#[derive(Debug, Clone)]
pub struct DbPersistence {
    pub tasks: TaskRepository,
    pub addresses: AddressRepository,
    pub referrals: ReferralRepository,

    pool: PgPool,
}

impl DbPersistence {
    pub async fn new(database_url: &str) -> DbResult<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await?;

        let tasks = TaskRepository::new(&pool);
        let addresses = AddressRepository::new(&pool);
        let referrals = ReferralRepository::new(&pool);

        Ok(Self {
            pool,
            tasks,
            addresses,
            referrals,
        })
    }
}
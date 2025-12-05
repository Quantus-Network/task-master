use sqlx::{postgres::PgPoolOptions, PgPool};

use crate::repositories::admin::AdminRepository;
use crate::repositories::eth_association::EthAssociationRepository;
use crate::repositories::relevant_tweet::RelevantTweetRepository;
use crate::repositories::x_association::XAssociationRepository;
use crate::repositories::DbResult;
use crate::repositories::{
    address::AddressRepository, opt_in::OptInRepository, referral::ReferralRepository, task::TaskRepository,
};

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
    #[error("Record not found: {0}")]
    RecordNotFound(String),
}

#[derive(Debug, Clone)]
pub struct DbPersistence {
    pub tasks: TaskRepository,
    pub addresses: AddressRepository,
    pub referrals: ReferralRepository,
    pub opt_ins: OptInRepository,
    pub x_associations: XAssociationRepository,
    pub eth_associations: EthAssociationRepository,
    pub admin: AdminRepository,
    pub relevant_tweets: RelevantTweetRepository,

    pub pool: PgPool,
}

impl DbPersistence {
    pub async fn new(database_url: &str) -> DbResult<Self> {
        let pool = PgPoolOptions::new().max_connections(10).connect(database_url).await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        let tasks = TaskRepository::new(&pool);
        let addresses = AddressRepository::new(&pool);
        let referrals = ReferralRepository::new(&pool);
        let opt_ins = OptInRepository::new(&pool);
        let x_associations = XAssociationRepository::new(&pool);
        let eth_associations = EthAssociationRepository::new(&pool);
        let admin = AdminRepository::new(&pool);
        let relevant_tweets = RelevantTweetRepository::new(&pool);

        Ok(Self {
            pool,
            tasks,
            addresses,
            referrals,
            opt_ins,
            x_associations,
            eth_associations,
            admin,
            relevant_tweets,
        })
    }

    #[cfg(test)]
    pub async fn new_unmigrated(database_url: &str) -> DbResult<Self> {
        let pool = PgPoolOptions::new().max_connections(5).connect(database_url).await?;

        let tasks = TaskRepository::new(&pool);
        let addresses = AddressRepository::new(&pool);
        let referrals = ReferralRepository::new(&pool);
        let opt_ins = OptInRepository::new(&pool);
        let x_associations = XAssociationRepository::new(&pool);
        let eth_associations = EthAssociationRepository::new(&pool);
        let admin = AdminRepository::new(&pool);
        let relevant_tweets = RelevantTweetRepository::new(&pool);

        Ok(Self {
            pool,
            tasks,
            addresses,
            referrals,
            opt_ins,
            x_associations,
            eth_associations,
            admin,
            relevant_tweets,
        })
    }
}

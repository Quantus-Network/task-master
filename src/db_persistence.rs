use sqlx::{postgres::PgPoolOptions, PgPool};

use crate::repositories::admin::AdminRepository;
use crate::repositories::eth_association::EthAssociationRepository;
use crate::repositories::raid_leaderboard::RaidLeaderboardRepository;
use crate::repositories::raid_quest::RaidQuestRepository;
use crate::repositories::raid_submission::RaidSubmissionRepository;
use crate::repositories::relevant_tweet::RelevantTweetRepository;
use crate::repositories::tweet_author::TweetAuthorRepository;
use crate::repositories::tweet_pull_usage::TweetPullUsageRepository;
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
    #[error("Record not found: {0}")]
    RecordNotFound(String),
    #[error("Conflict error: {0}")]
    UniqueViolation(String),
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
    pub tweet_authors: TweetAuthorRepository,
    pub raid_quests: RaidQuestRepository,
    pub raid_submissions: RaidSubmissionRepository,
    pub raid_leaderboards: RaidLeaderboardRepository,
    pub tweet_pull_usage: TweetPullUsageRepository,

    #[allow(unused_variables)]
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
        let tweet_authors = TweetAuthorRepository::new(&pool);
        let raid_quests = RaidQuestRepository::new(&pool);
        let raid_submissions = RaidSubmissionRepository::new(&pool);
        let raid_leaderboards = RaidLeaderboardRepository::new(&pool);
        let tweet_pull_usage = TweetPullUsageRepository::new(pool.clone());

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
            tweet_authors,
            raid_quests,
            raid_submissions,
            raid_leaderboards,
            tweet_pull_usage,
        })
    }
}

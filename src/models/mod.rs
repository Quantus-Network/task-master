#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("Invalid data input")]
    InvalidInput,
    #[error("Failed generating checkphrase")]
    FailedGenerateCheckphrase,
}

pub type ModelResult<T> = Result<T, ModelError>;

pub mod address;
pub mod admin;
pub mod auth;
pub mod eth_association;
pub mod opt_in;
pub mod raid_leaderboard;
pub mod raid_quest;
pub mod raid_submission;
pub mod referrals;
pub mod relevant_tweet;
pub mod task;
pub mod tweet_author;
pub mod tweet_pull_usage;
pub mod x_association;

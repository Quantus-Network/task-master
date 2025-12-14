use std::sync::Arc;

use rusx::{
    resources::{tweet::TweetParams, TweetField},
    TwitterGateway,
};

use crate::{
    db_persistence::DbPersistence,
    models::raid_submission::{RaidSubmission, UpdateRaidSubmissionStats},
    AppError, AppResult, Config,
};

#[derive(Clone)]
pub struct RaidLeaderboardService {
    db: Arc<DbPersistence>,
    twitter_gateway: Arc<dyn TwitterGateway>,
    config: Arc<Config>,
}

impl RaidLeaderboardService {
    fn build_batched_tweet_queries(submissions: &[RaidSubmission]) -> Vec<Vec<String>> {
        // Twitter's limit for the get ids result
        const TWEET_GET_MAX_IDS: usize = 100;

        submissions
            .chunks(TWEET_GET_MAX_IDS)
            .map(|chunk| chunk.iter().map(|s| s.id.clone()).collect())
            .collect()
    }

    pub fn new(db: Arc<DbPersistence>, twitter_gateway: Arc<dyn TwitterGateway>, config: Arc<Config>) -> Self {
        Self {
            db,
            twitter_gateway,
            config,
        }
    }

    pub fn spawn_raid_leaderboard_synchronizer(&self) -> tokio::task::JoinHandle<AppResult<()>> {
        let service = self.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(service.config.get_raid_leaderboard_sync_interval());

            loop {
                interval.tick().await;
                tracing::info!("🔄 Background Worker: Starting Raid Leaderboard Sync...");

                match service.sync_raid_leaderboard().await {
                    Ok(_) => tracing::info!("✅ Sync Complete."),
                    Err(e) => tracing::error!("❌ Sync Failed: {:?}", e),
                }
            }
        })
    }

    pub async fn sync_raid_leaderboard(&self) -> Result<(), AppError> {
        let active_raid_quest = self.db.raid_quests.find_active().await?;

        let Some(active_raid_quest) = active_raid_quest else {
            tracing::info!("No active raid quest at the moment, exiting...");
            return Ok(());
        };

        let raid_submissions = self.db.raid_submissions.find_by_raid(active_raid_quest.id).await?;
        if raid_submissions.is_empty() {
            tracing::info!("No raid submissions found yet for current active raid quest.");
            return Ok(Default::default());
        };

        let queries = RaidLeaderboardService::build_batched_tweet_queries(&raid_submissions);

        let mut params = TweetParams::new();
        params.tweet_fields = Some(vec![
            TweetField::PublicMetrics,
            TweetField::CreatedAt,
            TweetField::AuthorId,
        ]);

        // X Api Request Limit: 15 requests / 15 mins.
        // We set interval to 1 min (~1 req/min) to be safe.
        let mut rate_limiter = tokio::time::interval(self.config.get_raid_leaderboard_tweets_req_interval());

        for query in queries {
            rate_limiter.tick().await;

            let response = self
                .twitter_gateway
                .tweets()
                .get_many(query, Some(params.clone()))
                .await?;
            let Some(tweets) = &response.data else {
                tracing::info!("No tweets found!.");
                continue;
            };

            let updates: Vec<UpdateRaidSubmissionStats> = tweets
                .iter()
                .map(|t| UpdateRaidSubmissionStats::from(t.clone()))
                .collect();
            self.db.raid_submissions.update_stats_many(&updates).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::models::raid_quest::CreateRaidQuest;
    use crate::utils::test_db::reset_database;
    use mockall::predicate::*;
    use mockall::*;
    use rusx::{
        resources::{
            tweet::{Tweet, TweetApi, TweetPublicMetrics},
            TwitterApiResponse,
        },
        MockTweetApi, MockTwitterGateway,
    };
    use sqlx::PgPool;
    use std::sync::Arc;

    // -------------------------------------------------------------------------
    // Setup & Helpers
    // -------------------------------------------------------------------------

    async fn setup_deps() -> (Arc<DbPersistence>, Arc<Config>) {
        let config = Config::load_test_env().expect("Failed to load test config");
        let pool = PgPool::connect(config.get_database_url()).await.unwrap();
        reset_database(&pool).await;
        let db = Arc::new(DbPersistence::new(config.get_database_url()).await.unwrap());
        (db, Arc::new(config))
    }

    fn create_mock_tweet(id: &str, impressions: u32, likes: u32) -> Tweet {
        Tweet {
            id: id.to_string(),
            text: "Raid content".to_string(),
            author_id: Some("author_1".to_string()),
            created_at: Some(chrono::Utc::now().to_rfc3339()),
            public_metrics: Some(TweetPublicMetrics {
                impression_count: impressions,
                like_count: likes,
                reply_count: 5,
                retweet_count: 2,
                ..Default::default()
            }),
        }
    }

    // Helper to seed the DB requirements for a submission
    async fn seed_submission(db: &Arc<DbPersistence>, raid_id: i32, submission_id: &str) {
        // 1. Seed Raider (Address)
        let raider_id = "0xRaider";
        // Handle constraint if address already exists from previous calls in same test
        let _ = sqlx::query(
            "INSERT INTO addresses (quan_address, referral_code) VALUES ($1, 'REF') ON CONFLICT DO NOTHING",
        )
        .bind(raider_id)
        .execute(&db.pool)
        .await;

        // 2. Seed Tweet Author (Foreign Key for RelevantTweet)
        let _ = sqlx::query(
            "INSERT INTO tweet_authors (id, name, username) VALUES ('auth_1', 'Auth', 'auth') ON CONFLICT DO NOTHING",
        )
        .execute(&db.pool)
        .await;

        // 3. Seed Relevant Tweet (Target)
        let target_id = format!("target_{}", submission_id);
        let _ = sqlx::query(
            "INSERT INTO relevant_tweets (id, author_id, text, created_at) VALUES ($1, 'auth_1', 'Target', NOW())",
        )
        .bind(&target_id)
        .execute(&db.pool)
        .await;

        // 4. Create Submission
        let _ = sqlx::query(
            "INSERT INTO raid_submissions (id, raid_id, target_id, raider_id, text, impression_count, like_count) 
             VALUES ($1, $2, $3, $4, 'I joined!', 0, 0)",
        )
        .bind(submission_id)
        .bind(raid_id)
        .bind(target_id)
        .bind(raider_id)
        .execute(&db.pool)
        .await
        .unwrap();
    }

    // -------------------------------------------------------------------------
    // 3. Tests
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_sync_no_active_raid_does_nothing() {
        let (db, config) = setup_deps().await;

        // Setup Gateway: Expect NO calls because there is no active raid
        let mut mock_gateway = MockTwitterGateway::new();
        mock_gateway.expect_tweets().times(0);

        let service = RaidLeaderboardService::new(db, Arc::new(mock_gateway), config);

        let result = service.sync_raid_leaderboard().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_sync_active_raid_but_no_submissions_does_nothing() {
        let (db, config) = setup_deps().await;

        // 1. Create Active Raid
        db.raid_quests
            .create(&CreateRaidQuest {
                name: "Active Raid".to_string(),
                start_date: None,
                end_date: None,
            })
            .await
            .unwrap();

        // Setup Gateway: Expect NO calls because there are no submissions to check
        let mut mock_gateway = MockTwitterGateway::new();
        mock_gateway.expect_tweets().times(0);

        let service = RaidLeaderboardService::new(db, Arc::new(mock_gateway), config);

        let result = service.sync_raid_leaderboard().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_sync_updates_stats_successfully() {
        let (db, config) = setup_deps().await;

        // 1. Create Active Raid
        let raid_id = db
            .raid_quests
            .create(&CreateRaidQuest {
                name: "Active Raid".to_string(),
                start_date: None,
                end_date: None,
            })
            .await
            .unwrap();

        // 2. Seed Submission (Initial Stats: 0 impressions, 0 likes)
        let sub_id = "12345_submission";
        seed_submission(&db, raid_id, sub_id).await;

        // 3. Setup Mocks
        let mut mock_gateway = MockTwitterGateway::new();
        let mut mock_tweet_api = MockTweetApi::new();

        // Expect get_many to be called with the submission ID
        mock_tweet_api
            .expect_get_many()
            .with(predicate::eq(vec![sub_id.to_string()]), predicate::always())
            .times(1)
            .returning(|_, _| {
                Ok(TwitterApiResponse {
                    // Return UPDATED stats (100 impressions, 50 likes)
                    data: Some(vec![create_mock_tweet("12345_submission", 100, 50)]),
                    includes: None,
                    meta: None,
                })
            });

        mock_gateway
            .expect_tweets()
            .return_const(Arc::new(mock_tweet_api) as Arc<dyn TweetApi>);

        let service = RaidLeaderboardService::new(db.clone(), Arc::new(mock_gateway), config);

        // 4. Run Sync
        service.sync_raid_leaderboard().await.unwrap();

        // 5. Verify DB Updated
        let updated_sub = db.raid_submissions.find_by_id(sub_id).await.unwrap().unwrap();

        assert_eq!(updated_sub.impression_count, 100);
        assert_eq!(updated_sub.like_count, 50);
        assert!(updated_sub.updated_at > updated_sub.created_at);
    }

    #[tokio::test]
    async fn test_sync_batching_logic() {
        // This test verifies that if we have > 100 submissions,
        // the service makes multiple calls to Twitter.
        let (db, config) = setup_deps().await;

        let raid_id = db
            .raid_quests
            .create(&CreateRaidQuest {
                name: "Big Raid".to_string(),
                start_date: None,
                end_date: None,
            })
            .await
            .unwrap();

        // 1. Seed 150 Submissions
        // We just need unique IDs.
        let mut all_ids = Vec::new();
        for i in 0..150 {
            let id = format!("sub_{}", i);
            seed_submission(&db, raid_id, &id).await;
            all_ids.push(id);
        }

        // 2. Setup Mocks
        let mut mock_gateway = MockTwitterGateway::new();
        let mut mock_tweet_api = MockTweetApi::new();

        // We expect `get_many` to be called 2 times.
        // 1st time: 100 IDs
        // 2nd time: 50 IDs
        mock_tweet_api.expect_get_many().times(2).returning(|ids, _| {
            // Return valid responses for whatever IDs were requested
            let tweets = ids.iter().map(|id| create_mock_tweet(id, 10, 1)).collect();
            Ok(TwitterApiResponse {
                data: Some(tweets),
                includes: None,
                meta: None,
            })
        });

        mock_gateway
            .expect_tweets()
            .times(2)
            .return_const(Arc::new(mock_tweet_api) as Arc<dyn TweetApi>);

        let service = RaidLeaderboardService::new(db, Arc::new(mock_gateway), config);

        // 3. Run Sync

        service.sync_raid_leaderboard().await.unwrap();
    }
}

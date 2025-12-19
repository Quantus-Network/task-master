use std::{collections::HashMap, sync::Arc};

use rusx::{
    resources::{
        search::{SearchParams, SearchSortOrder},
        tweet::Tweet,
        TweetExpansion, TweetField, TwitterApiResponse, UserField,
    },
    TwitterGateway,
};

use crate::{
    db_persistence::DbPersistence,
    models::{relevant_tweet::NewTweetPayload, tweet_author::NewAuthorPayload},
    services::telegram_service::TelegramService,
    utils::x_url::build_x_status_url,
    AppError, AppResult, Config,
};

#[derive(Clone)]
pub struct TweetSynchronizerService {
    db: Arc<DbPersistence>,
    twitter_gateway: Arc<dyn TwitterGateway>,
    telegram_service: Arc<TelegramService>,
    config: Arc<Config>,
}

impl TweetSynchronizerService {
    async fn process_tweet_authors(
        &self,
        response: &TwitterApiResponse<Vec<Tweet>>,
    ) -> Result<Vec<NewAuthorPayload>, AppError> {
        let Some(includes) = &response.includes else {
            tracing::info!("No authors found in includes.");
            return Ok(Default::default());
        };
        let Some(authors) = &includes.users else {
            tracing::info!("No users found in includes.");
            return Ok(Default::default());
        };

        let authors_found_count = authors.len();
        tracing::info!("Found {} new authors. Saving...", authors_found_count);

        let new_authors: Vec<NewAuthorPayload> = authors
            .iter()
            .map(|author| NewAuthorPayload::new(author.clone()))
            .collect();

        if !new_authors.is_empty() {
            self.db.tweet_authors.upsert_many(&new_authors).await?;
            tracing::info!("Success saving {} new authors.", authors_found_count);
        }

        Ok(new_authors)
    }

    async fn process_relevant_tweets(
        &self,
        response: &TwitterApiResponse<Vec<Tweet>>,
    ) -> Result<Vec<NewTweetPayload>, AppError> {
        let Some(tweets) = &response.data else {
            tracing::info!("No new relevant tweets found.");
            return Ok(Default::default());
        };

        let tweets_found_count = tweets.len();
        tracing::info!("Found {} new relevant tweets. Saving...", tweets_found_count);

        let new_relevant_tweets: Vec<NewTweetPayload> =
            tweets.iter().map(|tweet| NewTweetPayload::new(tweet.clone())).collect();

        if !new_relevant_tweets.is_empty() {
            self.db.relevant_tweets.upsert_many(&new_relevant_tweets).await?;
            tracing::info!("Success saving {} new relevant tweets.", tweets_found_count);
        }

        Ok(new_relevant_tweets)
    }

    async fn process_sending_raid_targets(
        &self,
        tweet_authors: &[NewAuthorPayload],
        relevant_tweets: &[NewTweetPayload],
    ) -> Result<(), AppError> {
        if relevant_tweets.is_empty() {
            return Ok(());
        }

        let active_raid = self.db.raid_quests.find_active().await?;

        if active_raid.is_some() {
            tracing::info!("Active raid quests found, sending raid targets...");

            let author_lookup: HashMap<&String, &String> = tweet_authors.iter().map(|a| (&a.id, &a.username)).collect();
            let telegram_service = self.telegram_service.clone();
            let tweets_to_process = relevant_tweets.to_vec();
            let mut messages: Vec<String> = Vec::with_capacity(tweets_to_process.len());

            for tweet in tweets_to_process.iter().rev() {
                let author_name = match author_lookup.get(&tweet.author_id) {
                    Some(name) => name,
                    None => {
                        tracing::warn!(
                            "Author ID {} not found in lookup. Skipping notification.",
                            tweet.author_id
                        );
                        "Unknown"
                    }
                };

                let link = build_x_status_url(author_name, &tweet.id);

                let tg_message = format!(
                    "Raid Target Found!\n\n*Link*: {}\n*Author*: {}\n*Text*: {}\n*Impressions*: {}\n*Posted At*: {}",
                    link,
                    TelegramService::escape_markdown_v2(author_name),
                    TelegramService::escape_markdown_v2(&tweet.text),
                    tweet.impression_count,
                    tweet.created_at
                );
                messages.push(tg_message);
            }

            tokio::spawn(async move {
                // Telegram Limit: ~30 messages per second.
                // We set interval to 50ms (~20 msgs/sec) to be safe.
                let mut rate_limiter = tokio::time::interval(tokio::time::Duration::from_millis(50));

                for msg in messages {
                    rate_limiter.tick().await;

                    if let Err(e) = telegram_service.send_message(&msg).await {
                        tracing::error!("Failed to send raid notification: {:?}", e);
                    }
                }
            });
        }

        Ok(())
    }

    pub fn new(
        db: Arc<DbPersistence>,
        twitter_gateway: Arc<dyn TwitterGateway>,
        telegram_service: Arc<TelegramService>,
        config: Arc<Config>,
    ) -> Self {
        Self {
            db,
            twitter_gateway,
            telegram_service,
            config,
        }
    }

    pub fn spawn_tweet_synchronizer(&self) -> tokio::task::JoinHandle<AppResult<()>> {
        let service = self.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(service.config.get_tweet_sync_interval());

            loop {
                interval.tick().await;
                tracing::info!("🔄 Background Worker: Starting Twitter Sync...");

                match service.sync_relevant_tweets().await {
                    Ok(_) => tracing::info!("✅ Sync Complete."),
                    Err(e) => tracing::error!("❌ Sync Failed: {:?}", e),
                }
            }
        })
    }

    pub async fn sync_relevant_tweets(&self) -> Result<(), AppError> {
        let last_id = self.db.relevant_tweets.get_newest_tweet_id().await?;

        let whitelist_queries = SearchParams::build_batched_whitelist_queries(
            &self.config.tweet_sync.whitelist,
            Some(&self.config.tweet_sync.keywords),
        );

        for query in whitelist_queries {
            let mut params = SearchParams::new(query);
            params.max_results = Some(100);

            params.sort_order = Some(SearchSortOrder::Recency);

            params.tweet_fields = Some(vec![
                TweetField::PublicMetrics,
                TweetField::CreatedAt,
                TweetField::AuthorId,
            ]);
            params.user_fields = Some(vec![
                UserField::Username,
                UserField::Name,
                UserField::Id,
                UserField::PublicMetrics,
            ]);
            params.expansions = Some(vec![TweetExpansion::AuthorId]);

            if let Some(id) = last_id.clone() {
                params.since_id = Some(id.clone());
                tracing::info!("Syncing tweets since ID: {}", id);
            } else {
                tracing::info!("No history found, performing full 7-day fetch.");
            }

            let response = self.twitter_gateway.search().recent(params).await?;

            let tweet_authors = self.process_tweet_authors(&response).await?;
            let relevant_tweets = self.process_relevant_tweets(&response).await?;

            self.process_sending_raid_targets(&tweet_authors, &relevant_tweets)
                .await?;
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
    use rusx::{
        resources::{
            search::SearchApi,
            tweet::{Tweet, TweetPublicMetrics},
            user::{User, UserPublicMetrics},
            Includes, TwitterApiResponse,
        },
        MockSearchApi, MockTwitterGateway,
    };
    use sqlx::PgPool;
    use std::sync::Arc;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // -------------------------------------------------------------------------
    // Test Helpers
    // -------------------------------------------------------------------------

    async fn setup_deps() -> (Arc<DbPersistence>, MockServer, Arc<TelegramService>, Arc<Config>) {
        // A. Setup DB
        let config = Config::load_test_env().expect("Failed to load test config");
        let pool = PgPool::connect(config.get_database_url()).await.unwrap();
        reset_database(&pool).await;
        let db = Arc::new(DbPersistence::new(config.get_database_url()).await.unwrap());

        // B. Setup Telegram Mock Server
        let mock_server = MockServer::start().await;
        let telegram_service = Arc::new(TelegramService::new(
            &mock_server.uri(),
            "123456",
            &config.tg_bot.chat_id,
        ));

        // C. Config
        let app_config = Arc::new(config);

        (db, mock_server, telegram_service, app_config)
    }

    fn create_mock_tweet(id: &str, author_id: &str) -> Tweet {
        Tweet {
            id: id.to_string(),
            text: "Hello World".to_string(),
            author_id: Some(author_id.to_string()),
            created_at: Some(chrono::Utc::now().to_rfc3339()),
            in_reply_to_user_id: None,
            referenced_tweets: None,
            public_metrics: Some(TweetPublicMetrics {
                impression_count: 100,
                like_count: 10,
                reply_count: 5,
                retweet_count: 2,
                ..Default::default()
            }),
        }
    }

    fn create_mock_user(id: &str, username: &str) -> User {
        User {
            id: id.to_string(),
            name: "Test User".to_string(),
            username: username.to_string(),
            public_metrics: Some(UserPublicMetrics {
                followers_count: 1000,
                following_count: 100,
                tweet_count: 50,
                listed_count: 5,
                media_count: Some(0),
                like_count: Some(0),
            }),
        }
    }

    // -------------------------------------------------------------------------
    // Tests
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_sync_saves_data_no_raid_notification() {
        let (db, _mock_tg, telegram_service, config) = setup_deps().await;

        // --- Setup Mocks ---
        let mut mock_gateway = MockTwitterGateway::new();
        let mut mock_search = MockSearchApi::new();

        // Expect search().recent() to be called
        mock_search
            .expect_recent()
            .times(1) // Expect 1 batch (based on whitelist loop)
            .returning(|_| {
                Ok(TwitterApiResponse::<Vec<Tweet>> {
                    data: Some(vec![create_mock_tweet("t1", "u1")]),
                    includes: Some(Includes {
                        users: Some(vec![create_mock_user("u1", "user_one")]),
                        tweets: None,
                    }),
                    meta: None,
                })
            });

        let search_api_arc: Arc<dyn SearchApi> = Arc::new(mock_search);

        mock_gateway.expect_search().times(1).return_const(search_api_arc);

        // --- Run Service ---
        let service = TweetSynchronizerService::new(db.clone(), Arc::new(mock_gateway), telegram_service, config);

        let result = service.sync_relevant_tweets().await;

        // --- Assertions ---
        assert!(result.is_ok());

        // 1. Check DB for Authors
        let author = db.tweet_authors.find_by_id("u1").await.unwrap();
        assert!(author.is_some());
        assert_eq!(author.unwrap().username, "user_one");

        // 2. Check DB for Tweets
        let tweet = db.relevant_tweets.find_by_id("t1").await.unwrap();
        assert!(tweet.is_some());
        assert_eq!(tweet.unwrap().impression_count, 100);
    }

    #[tokio::test]
    async fn test_sync_sends_telegram_when_raid_active() {
        let (db, mock_tg, telegram_service, config) = setup_deps().await;

        // --- Setup Active Raid ---
        db.raid_quests
            .create(&CreateRaidQuest {
                name: "Test Raid".to_string(),
            })
            .await
            .unwrap();

        // --- Setup Telegram Mock ---
        // Expect a POST to /sendMessage
        Mock::given(method("POST"))
            .and(path("/bot123456/sendMessage"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1) // We expect 1 message for 1 tweet
            .mount(&mock_tg)
            .await;

        // --- Setup Twitter Mock ---
        let mut mock_gateway = MockTwitterGateway::new();
        let mut mock_search = MockSearchApi::new();

        mock_search.expect_recent().returning(|_| {
            Ok(TwitterApiResponse {
                data: Some(vec![create_mock_tweet("t2", "u2")]),
                includes: Some(Includes {
                    users: Some(vec![create_mock_user("u2", "user_two")]),
                    tweets: None,
                }),
                meta: None,
            })
        });

        let search_api_arc: Arc<dyn SearchApi> = Arc::new(mock_search);

        mock_gateway.expect_search().times(1).return_const(search_api_arc);

        // --- Run Service ---
        let service = TweetSynchronizerService::new(db.clone(), Arc::new(mock_gateway), telegram_service, config);

        // This will spawn the background task for telegram, so we need to wait slightly
        // or ensure the service function awaits the spawn (it doesn't in your code).
        // *Correction*: Your code uses `tokio::spawn` inside `process_sending_raid_targets`.
        // We need to verify the side effect (wiremock receiving request).
        service.sync_relevant_tweets().await.unwrap();

        // Wait a bit for the spawned tokio task to fire the http request
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    #[tokio::test]
    async fn test_pagination_logic_uses_last_id() {
        let (db, _mock_tg, telegram_service, config) = setup_deps().await;

        // --- Setup DB with existing tweet ---
        // We insert a tweet directly to simulate "last state"
        // Note: We need a valid author first due to FK
        db.tweet_authors
            .upsert_many(&vec![crate::models::tweet_author::NewAuthorPayload {
                id: "old_u".to_string(),
                name: "Old".to_string(),
                username: "old".to_string(),
                followers_count: 0,
                following_count: 0,
                tweet_count: 0,
                listed_count: 0,
                like_count: 0,
                media_count: 0,
            }])
            .await
            .unwrap();

        db.relevant_tweets
            .upsert_many(&vec![crate::models::relevant_tweet::NewTweetPayload {
                id: "12345".to_string(), // This is the ID we expect in 'since_id'
                author_id: "old_u".to_string(),
                text: "Old tweet".to_string(),
                impression_count: 0,
                reply_count: 0,
                retweet_count: 0,
                like_count: 0,
                created_at: chrono::Utc::now(),
            }])
            .await
            .unwrap();

        // --- Setup Twitter Mock ---
        let mut mock_gateway = MockTwitterGateway::new();
        let mut mock_search = MockSearchApi::new();

        // Verify the params passed to search contain the correct `since_id`
        mock_search
            .expect_recent()
            .withf(|params| {
                // Assert that since_id matches the one in DB
                params.since_id == Some("12345".to_string())
            })
            .returning(|_| {
                Ok(TwitterApiResponse {
                    data: None,
                    includes: None,
                    meta: None,
                })
            });

        let search_api_arc: Arc<dyn SearchApi> = Arc::new(mock_search);

        mock_gateway.expect_search().times(1).return_const(search_api_arc);

        let service = TweetSynchronizerService::new(db, Arc::new(mock_gateway), telegram_service, config);

        service.sync_relevant_tweets().await.unwrap();
    }
}

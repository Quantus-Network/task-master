use std::{collections::HashMap, sync::Arc}; // Optional: Makes multiline strings much cleaner

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

            for tweet in &tweets_to_process {
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

                let link = format!("https://x.com/{}/status/{}", author_name, &tweet.id);

                let tg_message = format!(
                    "Raid Target Found!\n\nLink: {}\nAuthor: {}\nText: {}\nImpressions: {}\nPosted At: {}",
                    link, author_name, tweet.text, tweet.impression_count, tweet.created_at
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

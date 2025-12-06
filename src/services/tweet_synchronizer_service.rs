use std::sync::Arc;

use rusx::{
    error::SdkError,
    resources::{
        search::{SearchParams, SearchSortOrder},
        tweet::Tweet,
        TweetExpansion, TweetField, TwitterApiResponse, UserField,
    },
    TwitterGateway,
};

use crate::{
    db_persistence::DbPersistence,
    models::{revelant_tweet::NewTweetPayload, tweet_author::NewAuthorPayload},
    AppError, AppResult, Config,
};

#[derive(Clone)]
pub struct TweetSynchronizerService {
    db: Arc<DbPersistence>,
    twitter_gateway: Arc<dyn TwitterGateway>,
    config: Arc<Config>,
}

impl TweetSynchronizerService {
    async fn process_tweet_authors(&self, response: TwitterApiResponse<Vec<Tweet>>) -> Result<(), AppError> {
        let authors = response
            .includes
            .ok_or_else(|| SdkError::Unknown("Tweet authors not found".to_string()))?
            .users
            .ok_or_else(|| SdkError::Unknown("Tweet authors not found".to_string()))?;
        let authors_found_count = authors.len();

        tracing::info!("Found {} new authors. Saving...", authors_found_count);
        let mut new_authors: Vec<NewAuthorPayload> = vec![];

        for author in authors {
            let new_author = NewAuthorPayload::new(author);
            new_authors.push(new_author);
        }

        self.db.tweet_authors.upsert_many(new_authors).await?;
        tracing::info!("Success saving {} new authors.", authors_found_count);

        Ok(())
    }

    async fn process_relevant_tweets(&self, response: TwitterApiResponse<Vec<Tweet>>) -> Result<(), AppError> {
        let tweets = response
            .data
            .ok_or_else(|| SdkError::Unknown("Tweet authors not found".to_string()))?;
        let tweets_found_count = tweets.len();

        tracing::info!("Found {} new relevant tweets. Saving...", tweets_found_count);
        let mut new_relevant_tweets: Vec<NewTweetPayload> = vec![];

        for tweet in tweets {
            let new_tweet = NewTweetPayload::new(tweet);
            new_relevant_tweets.push(new_tweet);
        }

        self.db.relevant_tweets.upsert_many(new_relevant_tweets).await?;
        tracing::info!("Success saving {} new relevant tweets.", tweets_found_count);

        Ok(())
    }

    pub fn new(db: Arc<DbPersistence>, twitter_gateway: Arc<dyn TwitterGateway>, config: Arc<Config>) -> Self {
        Self {
            db,
            twitter_gateway,
            config,
        }
    }

    pub fn spawn_tweet_synchronizer(&self) -> tokio::task::JoinHandle<AppResult<()>> {
        let service = self.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(service.config.get_tweet_sync_interval());

            loop {
                // Wait for the next tick
                // Note: The first tick completes immediately, so it runs on startup.
                // If you want to wait first, call interval.reset() or skip the first tick.
                interval.tick().await;

                tracing::info!("🔄 Background Worker: Starting Twitter Sync...");

                // Perform the Sync
                // We use match to handle errors without crashing the thread
                match service.sync_relevant_tweets().await {
                    Ok(_) => {
                        tracing::info!("✅ Sync Complete.");
                    }
                    Err(e) => {
                        tracing::info!("❌ Sync Failed: {:?}", e);
                    }
                }
            }
        })
    }

    pub async fn sync_relevant_tweets(&self) -> Result<(), AppError> {
        let last_id = self.db.relevant_tweets.get_newest_tweet_id().await?;

        let mut params = SearchParams::build_whitelist_query(
            &self.config.tweet_sync.whitelist,
            Some(&self.config.tweet_sync.keywords),
        );
        params.max_results = Some(100);
        params.sort_order = Some(SearchSortOrder::Relevancy);
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

        // CRITICAL: Only ask X for tweets newer than what we have
        if let Some(id) = last_id {
            params.since_id = Some(id.clone());
            tracing::info!("Syncing tweets since ID: {}", id);
        } else {
            tracing::info!("No history found, performing full 7-day fetch.");
        }

        let response = self.twitter_gateway.search().recent(params).await?;
        self.process_tweet_authors(response.clone()).await?;
        self.process_relevant_tweets(response).await?;

        Ok(())
    }
}

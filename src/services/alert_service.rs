use crate::repositories::tweet_pull_usage::TweetPullUsageRepository;
use crate::{AppResult, Config};
use reqwest::Client;
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct AlertService {
    client: Client,
    webhook_url: String,
    config: Config,
    usage_repo: TweetPullUsageRepository,
}

#[derive(Serialize)]
struct WebhookPayload<'a> {
    text: &'a str,
}

impl AlertService {
    pub fn new(config: Config, usage_repo: TweetPullUsageRepository) -> Self {
        Self {
            client: Client::new(),
            webhook_url: config.alert.webhook_url.clone(),
            config,
            usage_repo,
        }
    }

    /// Increments Twitter API usage and sends an alert if the threshold is reached.
    pub async fn track_and_alert_usage(&self, tweets_pulled: i32) -> AppResult<()> {
        if tweets_pulled <= 0 {
            return Ok(());
        }

        match self
            .usage_repo
            .increment_usage(tweets_pulled, self.config.tweet_sync.reset_day)
            .await
        {
            Ok(usage) => {
                let current_total = usage.tweet_count as u32;
                if current_total >= self.config.tweet_sync.alert_threshold {
                    if let Err(e) = self
                        .send_twitter_limit_alert(current_total, self.config.tweet_sync.monthly_limit)
                        .await
                    {
                        tracing::error!("Failed to send Twitter limit alert: {:?}", e);
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to increment Twitter API usage: {:?}", e);
            }
        }

        Ok(())
    }

    /// Sends an alert when the Twitter API tweet pull limit is nearing its threshold.
    pub async fn send_twitter_limit_alert(&self, current_count: u32, limit: u32) -> AppResult<()> {
        let message = format!(
            "Twitter API Limit Warning: Current usage {} tweets pulled, Plan limit {} tweets ({:.1}% used).",
            current_count,
            limit,
            (current_count as f32 / limit as f32) * 100.0
        );

        self.send_webhook_alert(&message).await
    }

    pub async fn send_webhook_alert(&self, text: &str) -> AppResult<()> {
        let payload = WebhookPayload { text };

        let response = self.client.post(&self.webhook_url).json(&payload).send().await;

        match response {
            Ok(res) if res.status().is_success() => Ok(()),
            Ok(res) => {
                let status = res.status().as_u16();
                let body = res.text().await.unwrap_or_default();
                // Reusing Telegram error variant for now or we could add a new one.
                // Given the instructions, let's keep it simple or add a Generic error.
                Err(crate::AppError::Server(format!(
                    "Webhook alert failed with status {}: {}",
                    status, body
                )))
            }
            Err(e) => Err(crate::AppError::Server(format!("Webhook alert request failed: {}", e))),
        }
    }
}

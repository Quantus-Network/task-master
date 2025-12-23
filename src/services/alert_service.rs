use crate::AppResult;
use reqwest::Client;
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct AlertService {
    client: Client,
    webhook_url: String,
}

#[derive(Serialize)]
struct WebhookPayload<'a> {
    text: &'a str,
}

impl AlertService {
    pub fn new(webhook_url: String) -> Self {
        Self {
            client: Client::new(),
            webhook_url,
        }
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

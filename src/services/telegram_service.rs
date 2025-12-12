use reqwest::{Client, StatusCode};
use serde::Serialize;

use crate::{AppError, AppResult};

#[derive(Clone)]
pub struct TelegramService {
    client: Client,
    base_url: String,
    default_chat_id: String,
}

#[derive(Serialize)]
struct MessagePayload<'a> {
    chat_id: &'a str,
    text: &'a str,
    parse_mode: &'a str,
    disable_web_page_preview: bool,
}

impl TelegramService {
    pub fn new(token: &str, default_chat_id: &str) -> Self {
        Self {
            client: Client::new(),
            base_url: format!("https://api.telegram.org/bot{token}"),
            default_chat_id: default_chat_id.to_string(),
        }
    }

    pub async fn send_message(&self, text: &str) -> AppResult<()> {
        self.send(&self.default_chat_id, text).await
    }

    async fn send(&self, chat_id: &str, text: &str) -> AppResult<()> {
        let url = format!("{}/sendMessage", self.base_url);

        let payload = MessagePayload {
            chat_id,
            text,
            parse_mode: "HTML", // or "MarkdownV2"
            disable_web_page_preview: true,
        };

        let response = self.client.post(&url).json(&payload).send().await;
        match response {
            Ok(response) => {
                if !response.status().is_success() {
                    let status = response.status().as_u16();
                    let body = response.text().await.unwrap_or_default();

                    return Err(AppError::Telegram(status, body));
                }

                Ok(())
            }
            Err(err) => {
                let status = err
                    .status()
                    .unwrap_or_else(|| StatusCode::INTERNAL_SERVER_ERROR)
                    .as_u16();
                let body = err.to_string();

                Err(AppError::Telegram(status, body))
            }
        }
    }
}

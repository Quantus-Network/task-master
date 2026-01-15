use reqwest::{Client, StatusCode};
use serde::Serialize;

use crate::{config::TelegramBotConfig, AppError, AppResult};

#[derive(Clone)]
pub struct TelegramService {
    client: Client,
    base_url: String,
    default_chat_id: String,
    default_message_thread_id: Option<String>,
}

#[derive(Serialize)]
struct MessagePayload<'a> {
    chat_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    message_thread_id: Option<&'a str>,
    text: &'a str,
    parse_mode: &'a str,
    disable_web_page_preview: bool,
}

impl TelegramService {
    pub fn escape_markdown_v2(text: &str) -> String {
        text.replace("_", "\\_")
            .replace("*", "\\*")
            .replace("[", "\\[")
            .replace("]", "\\]")
            .replace("(", "\\(")
            .replace(")", "\\)")
            .replace("~", "\\~")
            .replace("`", "\\`")
            .replace(">", "\\>")
            .replace("#", "\\#")
            .replace("+", "\\+")
            .replace("-", "\\-")
            .replace("=", "\\=")
            .replace("|", "\\|")
            .replace("{", "\\{")
            .replace("}", "\\}")
            .replace(".", "\\.")
            .replace("!", "\\!")
    }

    pub fn new(config: TelegramBotConfig) -> Self {
        Self {
            client: Client::new(),
            base_url: format!("{}/bot{}", config.base_url, config.token),
            default_chat_id: config.chat_id,
            default_message_thread_id: config.message_thread_id,
        }
    }

    pub async fn send_message(&self, text: &str) -> AppResult<()> {
        self.send(&self.default_chat_id, self.default_message_thread_id.as_deref(), text)
            .await
    }

    async fn send(&self, chat_id: &str, message_thread_id: Option<&str>, text: &str) -> AppResult<()> {
        let url = format!("{}/sendMessage", self.base_url);

        let payload = MessagePayload {
            chat_id,
            message_thread_id,
            text,
            parse_mode: "MarkdownV2", // or "HTML"
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
                let status = err.status().unwrap_or(StatusCode::INTERNAL_SERVER_ERROR).as_u16();
                let body = err.to_string();

                Err(AppError::Telegram(status, body))
            }
        }
    }
}

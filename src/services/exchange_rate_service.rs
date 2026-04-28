use std::{
    collections::HashMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Deserialize;
use thiserror::Error;
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct ExchangeRateSnapshot {
    pub conversion_rates: HashMap<String, f64>,
    pub time_next_update_unix: i64,
}

#[derive(Debug, Error)]
pub enum ExchangeRateError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("exchange rate API error: {0}")]
    Api(String),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
pub struct ExchangeRateService {
    client: reqwest::Client,
    /// Full prefix including key, e.g. `https://v6.exchangerate-api.com/v6/{key}`.
    base_url: String,
    cache: Arc<Mutex<HashMap<String, ExchangeRateSnapshot>>>,
    base_currency: String,
}

impl ExchangeRateService {
    pub fn new(api_key: &str) -> Self {
        let base_url = format!("https://v6.exchangerate-api.com/v6/{}", api_key);
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            client,
            base_url,
            cache: Arc::new(Mutex::new(HashMap::new())),
            base_currency: "USD".to_string(),
        }
    }

    pub async fn get_snapshot(&self) -> Result<ExchangeRateSnapshot, ExchangeRateError> {
        let base = normalize_currency_code(&self.base_currency);

        let mut guard = self.cache.lock().await;
        if let Some(s) = guard.get(&base) {
            if cache_is_fresh(s) {
                return Ok(s.clone());
            }
        }

        let snapshot = self.fetch_latest(&base).await?;
        guard.insert(base, snapshot.clone());
        Ok(snapshot)
    }

    async fn fetch_latest(&self, base: &str) -> Result<ExchangeRateSnapshot, ExchangeRateError> {
        let url = format!("{}/latest/{}", self.base_url, base);
        let response = self.client.get(&url).send().await?.error_for_status()?;
        let text = response.text().await?;
        let parsed: ExchangeRateApiV6Response = serde_json::from_str(&text)?;

        if parsed.result != "success" {
            let detail = parsed.error_type.unwrap_or_else(|| "unknown error".to_string());
            return Err(ExchangeRateError::Api(format!("{}: {}", parsed.result, detail)));
        }

        let conversion_rates = parsed
            .conversion_rates
            .ok_or_else(|| ExchangeRateError::Api("missing conversion_rates in success body".to_string()))?;
        let time_next = parsed
            .time_next_update_unix
            .ok_or_else(|| ExchangeRateError::Api("missing time_next_update_unix".to_string()))?;

        Ok(ExchangeRateSnapshot {
            conversion_rates,
            time_next_update_unix: i64::try_from(time_next).unwrap_or(i64::MAX),
        })
    }
}

#[cfg(test)]
impl ExchangeRateService {
    fn new_test(base_url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            client,
            base_url,
            cache: Arc::new(Mutex::new(HashMap::new())),
            base_currency: "USD".to_string(),
        }
    }
}

fn normalize_currency_code(s: &str) -> String {
    s.trim().to_uppercase()
}

fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn cache_is_fresh(snapshot: &ExchangeRateSnapshot) -> bool {
    if snapshot.time_next_update_unix <= 0 {
        return false;
    }
    now_unix_seconds() < snapshot.time_next_update_unix
}

#[derive(Deserialize)]
struct ExchangeRateApiV6Response {
    result: String,
    conversion_rates: Option<HashMap<String, f64>>,
    time_next_update_unix: Option<u64>,
    #[allow(dead_code)]
    time_last_update_unix: Option<u64>,
    #[serde(rename = "error-type", default)]
    error_type: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const MOCK_FUTURE_NEXT: i64 = 4_000_000_000;

    fn success_body() -> String {
        serde_json::json!({
            "result": "success",
            "time_last_update_unix": 0,
            "time_next_update_unix": MOCK_FUTURE_NEXT,
            "base_code": "USD",
            "conversion_rates": {
                "USD": 1.0,
                "EUR": 0.9
            }
        })
        .to_string()
    }

    #[test]
    fn normalize_uppercases_and_trims() {
        assert_eq!(normalize_currency_code(" eur ").as_str(), "EUR");
    }

    #[test]
    fn cache_fresh_honors_next_update() {
        let fresh = ExchangeRateSnapshot {
            conversion_rates: HashMap::new(),
            time_next_update_unix: MOCK_FUTURE_NEXT,
        };
        let stale = ExchangeRateSnapshot {
            conversion_rates: HashMap::new(),
            time_next_update_unix: 0,
        };
        assert!(cache_is_fresh(&fresh));
        assert!(!cache_is_fresh(&stale));
    }

    /// Second `get_snapshot` must not call the network again (wiremock would panic on 2+ hits if we set expect(1)).
    #[tokio::test]
    async fn cache_reuses_one_http_call() {
        let server = MockServer::start().await;
        let v6 = format!("{}/v6/test-key", server.uri());

        Mock::given(method("GET"))
            .and(path("/v6/test-key/latest/USD"))
            .respond_with(ResponseTemplate::new(200).set_body_string(success_body()))
            .expect(1)
            .mount(&server)
            .await;

        let service = ExchangeRateService::new_test(v6);
        let _a = service.get_snapshot().await.unwrap();
        let _b = service.get_snapshot().await.unwrap();
    }
}

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use lazy_static::lazy_static;
#[cfg(target_os = "linux")]
use prometheus::process_collector::ProcessCollector;
use prometheus::{Encoder, HistogramOpts, HistogramVec, IntCounterVec, IntGauge, Opts, Registry, TextEncoder};
use std::sync::Arc;
use std::time::Instant;

use crate::http_server::AppState;

// Define comprehensive metrics for REST API monitoring
lazy_static! {
    pub static ref HTTP_REQUESTS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("http_requests_total", "Total number of HTTP requests"),
        &["method", "endpoint", "status"]
    )
    .unwrap();
    pub static ref HTTP_REQUEST_DURATION: HistogramVec = HistogramVec::new(
        HistogramOpts::new("http_request_duration_seconds", "HTTP request duration in seconds").buckets(vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0
        ]),
        &["method", "endpoint"]
    )
    .unwrap();
    pub static ref HTTP_REQUESTS_IN_FLIGHT: IntGauge = IntGauge::new(
        "http_requests_in_flight",
        "Number of HTTP requests currently being processed"
    )
    .unwrap();
    pub static ref HTTP_REQUEST_SIZE_BYTES: HistogramVec = HistogramVec::new(
        HistogramOpts::new("http_request_size_bytes", "Size of HTTP request bodies in bytes")
            .buckets(vec![100.0, 1000.0, 10000.0, 100000.0, 1000000.0, 10000000.0]),
        &["method", "endpoint"]
    )
    .unwrap();
    pub static ref HTTP_RESPONSE_SIZE_BYTES: HistogramVec = HistogramVec::new(
        HistogramOpts::new("http_response_size_bytes", "Size of HTTP response bodies in bytes")
            .buckets(vec![100.0, 1000.0, 10000.0, 100000.0, 1000000.0, 10000000.0]),
        &["method", "endpoint"]
    )
    .unwrap();
    pub static ref HTTP_ERRORS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("http_errors_total", "Total number of HTTP errors"),
        &["method", "endpoint", "status"]
    )
    .unwrap();

    // Twitter API metrics
    pub static ref TWITTER_API_CALLS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("twitter_api_calls_total", "Total number of Twitter API calls"),
        &["operation"]
    )
    .unwrap();
    pub static ref TWITTER_API_CALL_DURATION: HistogramVec = HistogramVec::new(
        HistogramOpts::new("twitter_api_call_duration_seconds", "Twitter API call duration in seconds")
            .buckets(vec![0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0]),
        &["operation"]
    )
    .unwrap();
    pub static ref TWITTER_TWEETS_PULLED_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("twitter_tweets_pulled_total", "Total number of tweets pulled from Twitter API"),
        &["operation"]
    )
    .unwrap();
    pub static ref TWITTER_API_ERRORS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("twitter_api_errors_total", "Total number of Twitter API errors"),
        &["operation", "error_type"]
    )
    .unwrap();
    pub static ref TWITTER_TWEETS_PER_CALL: HistogramVec = HistogramVec::new(
        HistogramOpts::new("twitter_tweets_per_call", "Number of tweets returned per API call")
            .buckets(vec![0.0, 1.0, 5.0, 10.0, 25.0, 50.0, 100.0]),
        &["operation"]
    )
    .unwrap();
}

#[derive(Debug, Clone)]
pub struct Metrics {
    pub registry: Arc<Registry>,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    pub fn new() -> Self {
        let registry = Registry::new();

        // Register OS/machine metrics collector (Linux only)
        #[cfg(target_os = "linux")]
        {
            let process_collector = ProcessCollector::for_self();
            registry.register(Box::new(process_collector)).unwrap();
        }

        // Register all custom HTTP metrics
        registry.register(Box::new(HTTP_REQUESTS_TOTAL.clone())).unwrap();
        registry.register(Box::new(HTTP_REQUEST_DURATION.clone())).unwrap();
        registry.register(Box::new(HTTP_REQUESTS_IN_FLIGHT.clone())).unwrap();
        registry.register(Box::new(HTTP_REQUEST_SIZE_BYTES.clone())).unwrap();
        registry.register(Box::new(HTTP_RESPONSE_SIZE_BYTES.clone())).unwrap();
        registry.register(Box::new(HTTP_ERRORS_TOTAL.clone())).unwrap();

        // Register Twitter API metrics
        registry.register(Box::new(TWITTER_API_CALLS_TOTAL.clone())).unwrap();
        registry.register(Box::new(TWITTER_API_CALL_DURATION.clone())).unwrap();
        registry
            .register(Box::new(TWITTER_TWEETS_PULLED_TOTAL.clone()))
            .unwrap();
        registry.register(Box::new(TWITTER_API_ERRORS_TOTAL.clone())).unwrap();
        registry.register(Box::new(TWITTER_TWEETS_PER_CALL.clone())).unwrap();

        Self {
            registry: Arc::new(registry),
        }
    }
}

/// Helper to normalize endpoints (remove IDs, UUIDs, etc.)
/// Example: /users/123/posts/456?lang=en -> /users/:id/posts/:id
pub fn normalize_path(path: &str) -> String {
    // Remove query string if present
    let path_only = path.split('?').next().unwrap_or(path);

    let segments: Vec<&str> = path_only
        .split('/')
        .filter(|s| !s.is_empty()) // avoid leading/trailing empty segments
        .collect();

    segments
        .iter()
        .map(|s| {
            // Replace numeric IDs or UUIDs with ":id"
            if s.parse::<i64>().is_ok() || s.len() == 36 {
                ":id"
            } else {
                *s
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Middleware for tracking HTTP metrics
pub async fn track_metrics(req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();
    let method = req.method().clone();

    // Skip metrics for the metrics endpoint
    if path == "/metrics" {
        return next.run(req).await;
    }

    // Normalize endpoint for better cardinality
    let endpoint = normalize_path(&path);

    // Track request size if available
    if let Some(content_length) = req.headers().get("content-length") {
        if let Ok(size) = content_length.to_str().unwrap_or("0").parse::<f64>() {
            HTTP_REQUEST_SIZE_BYTES
                .with_label_values(&[method.as_str(), &endpoint])
                .observe(size);
        }
    }

    // Track in-flight requests
    HTTP_REQUESTS_IN_FLIGHT.inc();

    // Start timer
    let start = Instant::now();

    // Process request
    let response = next.run(req).await;

    // Record timing
    let duration = start.elapsed().as_secs_f64();
    HTTP_REQUEST_DURATION
        .with_label_values(&[method.as_str(), &endpoint])
        .observe(duration);

    // Track response status
    let status = response.status();
    let status_str = status.as_u16().to_string();

    HTTP_REQUESTS_TOTAL
        .with_label_values(&[method.as_str(), &endpoint, &status_str])
        .inc();

    // Track errors (4xx and 5xx)
    if status.is_client_error() || status.is_server_error() {
        HTTP_ERRORS_TOTAL
            .with_label_values(&[method.as_str(), &endpoint, &status_str])
            .inc();
    }

    // Track response size if available
    if let Some(content_length) = response.headers().get("content-length") {
        if let Ok(size) = content_length.to_str().unwrap_or("0").parse::<f64>() {
            HTTP_RESPONSE_SIZE_BYTES
                .with_label_values(&[method.as_str(), &endpoint])
                .observe(size);
        }
    }

    HTTP_REQUESTS_IN_FLIGHT.dec();

    response
}

/// Track Twitter API call metrics
///
/// This function should be called around Twitter API calls to track:
/// - API call duration
/// - Number of tweets pulled
/// - Errors
///
/// # Arguments
/// * `operation` - The type of operation (e.g., "search_recent", "tweets_get_many")
/// * `f` - The async function that makes the Twitter API call
///
/// # Returns
/// The result of the API call, with metrics automatically tracked
pub async fn track_twitter_api_call<T, E, F>(operation: &str, f: F) -> Result<T, E>
where
    F: std::future::Future<Output = Result<T, E>>,
{
    let start = Instant::now();

    // Track API call attempt
    TWITTER_API_CALLS_TOTAL.with_label_values(&[operation]).inc();

    let result = f.await;

    // Record duration
    let duration = start.elapsed().as_secs_f64();
    TWITTER_API_CALL_DURATION
        .with_label_values(&[operation])
        .observe(duration);

    // Track errors if any
    if result.is_err() {
        TWITTER_API_ERRORS_TOTAL
            .with_label_values(&[operation, "api_error"])
            .inc();
    }

    result
}

/// Track tweets pulled from a Twitter API call
///
/// Call this after successfully getting tweets from the API to track:
/// - Total tweets pulled
/// - Tweets per call distribution
///
/// # Arguments
/// * `operation` - The type of operation (e.g., "search_recent", "tweets_get_many")
/// * `tweet_count` - Number of tweets returned by the API call
pub fn track_tweets_pulled(operation: &str, tweet_count: usize) {
    if tweet_count > 0 {
        TWITTER_TWEETS_PULLED_TOTAL
            .with_label_values(&[operation])
            .inc_by(tweet_count as u64);
        TWITTER_TWEETS_PER_CALL
            .with_label_values(&[operation])
            .observe(tweet_count as f64);
    }
}

pub async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    let encoder = TextEncoder::new();
    let metric_families = state.metrics.registry.gather();

    let mut buffer = Vec::new();
    if let Err(e) = encoder.encode(&metric_families, &mut buffer) {
        eprintln!("Failed to encode metrics: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            String::from("Failed to encode metrics"),
        );
    }

    let res = match String::from_utf8(buffer) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("custom metrics could not be from_utf8'd: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                String::from("Failed to encode metrics"),
            );
        }
    };

    (StatusCode::OK, res)
}

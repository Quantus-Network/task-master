use axum::{routing::get, Router};

use crate::{handlers::risk_checker::handle_get_risk_report, http_server::AppState};

pub fn risk_checker_routes() -> Router<AppState> {
    Router::new().route("/risk-checker/:address_or_ens", get(handle_get_risk_report))
}

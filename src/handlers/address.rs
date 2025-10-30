use axum::{
    extract::{self, Query, State},
    response::NoContent,
    Extension, Json,
};

use crate::{
    db_persistence::DbError,
    handlers::{HandlerError, PaginationMetadata, QueryParams},
    http_server::AppState,
    models::address::{
        Address, AddressStatsResponse, AggregateStatsQueryParams, AssociateEthAddressRequest,
        AssociateEthAddressResponse, OptedInPositionResponse, PaginatedAddressesResponse,
        RewardProgramStatusPayload, SyncTransfersResponse,
    },
    AppError,
};

use super::SuccessResponse;

#[derive(Debug, thiserror::Error)]
pub enum AddressHandlerError {
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    InvalidQueryParams(String),
}

pub async fn handle_update_reward_program_status(
    State(state): State<AppState>,
    Extension(user): Extension<Address>,
    extract::Path(id): extract::Path<String>,
    extract::Json(payload): Json<RewardProgramStatusPayload>,
) -> Result<NoContent, AppError> {
    if user.quan_address.0 != id {
        return Err(AppError::Handler(HandlerError::Address(
            AddressHandlerError::Unauthorized(
                "You can only update your own reward program status".to_string(),
            ),
        )));
    }
    tracing::debug!("Updating address reward status to {}", payload.new_status);

    if payload.new_status {
        let opt_in_count = state.db.opt_ins.count().await?;
        let opt_in_number = (opt_in_count + 1) as i32;
        state.db.opt_ins.create(&id, opt_in_number).await?;
    } else {
        state.db.opt_ins.delete(&id).await?;
    }

    Ok(NoContent)
}

pub async fn handle_get_address_stats(
    State(state): State<AppState>,
    extract::Path(id): extract::Path<String>,
) -> Result<Json<SuccessResponse<AddressStatsResponse>>, AppError> {
    tracing::info!("Getting address stats...");

    let referrals = state.db.referrals.count_by_referrer(id.clone()).await? as u64;
    let stats = state.graphql_client.get_address_stats(id.clone()).await?;

    let data = AddressStatsResponse {
        referrals,
        referral_events: 0,
        immediate_txs: stats.total_transactions,
        reversible_txs: stats.total_reversible_transactions,
        mining_events: stats.total_mined_blocks,
        mining_rewards: stats.total_mining_rewards,
    };

    Ok(SuccessResponse::new(data))
}

pub async fn handle_aggregate_address_stats(
    State(state): State<AppState>,
    Extension(user): Extension<Address>,
    Query(params): Query<AggregateStatsQueryParams>,
) -> Result<Json<SuccessResponse<AddressStatsResponse>>, AppError> {
    tracing::info!("Aggregate addresses stats...");

    if params.addresses.is_empty() {
        return Err(AppError::Handler(HandlerError::Address(
            AddressHandlerError::InvalidQueryParams(
                "Addresses query parameter should be defined and not empty!".to_string(),
            ),
        )));
    }

    let referrals = state
        .db
        .referrals
        .find_all_by_referrer(user.quan_address.0)
        .await?;
    let referred_addresses: Vec<String> = referrals
        .iter()
        .map(|acc| acc.referee_address.0.clone())
        .collect();
    let referral_events = state
        .graphql_client
        .get_addresses_events_count(referred_addresses)
        .await?;
    let stats = state
        .graphql_client
        .get_addresses_stats(params.addresses.clone())
        .await?;

    let data = AddressStatsResponse {
        referrals: referrals.len() as u64,
        referral_events,
        immediate_txs: stats.total_transactions,
        reversible_txs: stats.total_reversible_transactions,
        mining_events: stats.total_mined_blocks,
        mining_rewards: stats.total_mining_rewards,
    };

    Ok(SuccessResponse::new(data))
}

pub async fn handle_get_leaderboard(
    State(state): State<AppState>,
    Query(params): Query<QueryParams>,
) -> Result<Json<PaginatedAddressesResponse>, AppError> {
    tracing::info!("Getting leadeboard data...");

    if params.page < 1 {
        return Err(AppError::Handler(HandlerError::QueryParams(
            "Page query params must not be less than 1".to_string(),
        )));
    }

    if params.page_size < 1 {
        return Err(AppError::Handler(HandlerError::QueryParams(
            "Page size query params must not be less than 1".to_string(),
        )));
    }

    let total_items = state.db.addresses.get_total_items().await? as u32;
    let total_pages = ((total_items as f64) / (params.page_size as f64)).ceil() as u32;
    let offset = (params.page - 1) * params.page_size;

    let addresses = state
        .db
        .addresses
        .get_leaderboard_entries(params.page_size, offset)
        .await?;

    let response = PaginatedAddressesResponse {
        data: addresses,
        meta: PaginationMetadata {
            page: params.page,
            page_size: params.page_size,
            total_items,
            total_pages,
        },
    };

    Ok(Json(response))
}

pub async fn handle_get_address_reward_status_by_id(
    State(state): State<AppState>,
    extract::Path(id): extract::Path<String>,
) -> Result<Json<SuccessResponse<bool>>, AppError> {
    tracing::info!("Getting address by id {}", id);

    if state.db.addresses.find_by_id(&id).await?.is_none() {
        return Err(AppError::Database(DbError::AddressNotFound(
            "Address not found!".to_string(),
        )));
    }

    let is_opted_in = state.db.opt_ins.find_by_address(&id).await?.is_some();
    Ok(SuccessResponse::new(is_opted_in))
}

pub async fn associate_eth_address(
    State(state): State<AppState>,
    Extension(user): Extension<Address>,
    Json(payload): Json<AssociateEthAddressRequest>,
) -> Result<Json<AssociateEthAddressResponse>, AppError> {
    tracing::info!(
        "Received ETH address association request for quan_address: {} -> eth_address: {}",
        user.quan_address.0,
        payload.eth_address,
    );

    // Update the user's ETH address
    match state
        .db
        .addresses
        .update_address_eth(&user.quan_address.0, &payload.eth_address)
        .await
    {
        Ok(_) => {
            tracing::info!(
                "Updated quan_address {} with eth_address {}",
                user.quan_address.0,
                payload.eth_address
            );
        }
        Err(db_err) => return Err(AppError::Database(db_err)),
    }

    let response = AssociateEthAddressResponse {
        success: true,
        message: "Ethereum address associated successfully".to_string(),
    };

    Ok(Json(response))
}

pub async fn sync_transfers(
    State(state): State<AppState>,
) -> Result<Json<SyncTransfersResponse>, AppError> {
    tracing::info!("Received request to sync transfers from GraphQL endpoint");

    match state.graphql_client.sync_transfers_and_addresses().await {
        Ok((transfer_count, address_count)) => {
            tracing::info!(
                "Transfer sync completed successfully: {} transfers, {} addresses",
                transfer_count,
                address_count
            );

            let response = SyncTransfersResponse {
                success: true,
                message: format!(
                    "Successfully processed {} transfers and stored {} addresses",
                    transfer_count, address_count
                ),
                transfers_processed: Some(transfer_count),
                addresses_stored: Some(address_count),
            };

            Ok(Json(response))
        }
        Err(e) => {
            tracing::error!("Failed to sync transfers: {}", e);

            Err(AppError::Graphql(e))
        }
    }
}

pub async fn handle_get_opted_in_users(
    State(state): State<AppState>,
) -> Result<Json<SuccessResponse<Vec<crate::models::opt_in::OptIn>>>, AppError> {
    tracing::info!("Getting first 100 opted-in users");

    let opt_ins = state.db.opt_ins.get_all_ordered(100).await?;

    Ok(SuccessResponse::new(opt_ins))
}

pub async fn handle_get_opted_in_position(
    State(state): State<AppState>,
    Extension(user): Extension<Address>,
) -> Result<Json<SuccessResponse<OptedInPositionResponse>>, AppError> {
    tracing::info!("Getting opted-in position for {}", user.quan_address.0);

    if let Some(opt_in) = state.db.opt_ins.find_by_address(&user.quan_address.0).await? {
        Ok(SuccessResponse::new(OptedInPositionResponse {
            quan_address: user.quan_address.0,
            position: opt_in.opt_in_number as i64,
            is_opted_in: true,
        }))
    } else {
        Ok(SuccessResponse::new(OptedInPositionResponse {
            quan_address: user.quan_address.0,
            position: 0,
            is_opted_in: false,
        }))
    }
}

#[cfg(test)]
mod tests {}

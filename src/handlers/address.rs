use axum::{
    extract::{self, State},
    response::NoContent,
    Extension, Json,
};

use crate::{
    db_persistence::DbError,
    handlers::HandlerError,
    http_server::AppState,
    models::{
        address::{
            Address, AddressStatsResponse, AssociateEthAddressRequest,
            AssociateEthAddressResponse, RewardProgramStatusPayload,
            SyncTransfersResponse,
        },
    },
    AppError,
};

use super::SuccessResponse;

#[derive(Debug, thiserror::Error)]
pub enum AddressHandlerError {
    #[error("{0}")]
    Unauthorized(String),
}

pub async fn handle_update_reward_program_status(
    State(state): State<AppState>,
    Extension(user): Extension<Address>,
    extract::Path(id): extract::Path<String>,
    extract::Json(payload): Json<RewardProgramStatusPayload>,
) -> Result<NoContent, AppError> {
    // Ensure the authenticated user can only update their own reward program status
    if user.quan_address.0 != id {
        return Err(AppError::Handler(HandlerError::Address(
            AddressHandlerError::Unauthorized("You can only update your own reward program status".to_string()),
        )));
    }
    tracing::debug!("Updating address reward status to {}", payload.new_status);

    state
        .db
        .addresses
        .update_address_reward_status(&id, payload.new_status)
        .await?;

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
        immediate_txs: stats.total_transactions,
        reversible_txs: stats.total_reversible_transactions,
        mining_events: stats.total_mined_blocks,
        mining_rewards: stats.total_mining_rewards,
    };

    Ok(SuccessResponse::new(data))
}

pub async fn handle_get_address_reward_status_by_id(
    State(state): State<AppState>,
    extract::Path(id): extract::Path<String>,
) -> Result<Json<SuccessResponse<bool>>, AppError> {
    tracing::info!("Getting address by id {}", id);

    if let Some(address) = state.db.addresses.find_by_id(&id).await? {
        Ok(SuccessResponse::new(address.is_reward_program_participant))
    } else {
        Err(AppError::Database(DbError::AddressNotFound(
            "Address not found!".to_string(),
        )))
    }
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

#[cfg(test)]
mod tests {
}

use axum::{
    extract::{Path, Query, State},
    response::NoContent,
    Extension, Json,
};

use crate::{
    handlers::{
        calculate_total_pages, validate_pagination_query, LeaderboardQueryParams, ListQueryParams, PaginatedResponse,
        PaginationMetadata,
    },
    http_server::AppState,
    models::{
        admin::Admin,
        raid_leaderboard::RaidLeaderboard,
        raid_quest::{CreateRaidQuest, RaidQuest, RaidQuestFilter, RaidQuestSortColumn},
    },
    AppError,
};

use super::SuccessResponse;

// -----------------------------------------------------------------------------
// Admin Handlers
// -----------------------------------------------------------------------------

/// Create a new Raid Quest (Admin Only)
pub async fn handle_create_raid(
    State(state): State<AppState>,
    Extension(_admin): Extension<Admin>,
    Json(payload): Json<CreateRaidQuest>,
) -> Result<Json<SuccessResponse<i32>>, AppError> {
    tracing::info!("Admin creating new raid: {}", payload.name);

    let raid_id = state.db.raid_quests.create(&payload).await?;

    Ok(SuccessResponse::new(raid_id))
}

/// Finish/End a Raid Quest (Admin Only)
pub async fn handle_finish_raid(
    State(state): State<AppState>,
    Extension(_admin): Extension<Admin>,
    Path(id): Path<i32>,
) -> Result<NoContent, AppError> {
    tracing::info!("Admin finishing raid id: {}", id);

    state.db.raid_quests.finish(id).await?;

    Ok(NoContent)
}

pub async fn handle_revert_to_active_raid(
    State(state): State<AppState>,
    Extension(_admin): Extension<Admin>,
    Path(id): Path<i32>,
) -> Result<NoContent, AppError> {
    tracing::info!("Admin finishing raid id: {}", id);

    state.db.raid_quests.make_active(id).await?;

    Ok(NoContent)
}

pub async fn handle_delete_raid(
    State(state): State<AppState>,
    Extension(_admin): Extension<Admin>,
    Path(id): Path<i32>,
) -> Result<NoContent, AppError> {
    tracing::info!("Admin finishing raid id: {}", id);

    state.db.raid_quests.delete_by_id(id).await?;

    Ok(NoContent)
}

// -----------------------------------------------------------------------------
// Public Handlers
// -----------------------------------------------------------------------------

/// Lists raid quests with pagination, and filtering.
pub async fn handle_get_raid_quests(
    State(state): State<AppState>,
    Query(params): Query<ListQueryParams<RaidQuestSortColumn>>,
    Query(filters): Query<RaidQuestFilter>,
) -> Result<Json<PaginatedResponse<RaidQuest>>, AppError> {
    validate_pagination_query(params.page, params.page_size)?;

    let total_items = state.db.raid_quests.count_filtered(&params, &filters).await? as u32;
    let total_pages = calculate_total_pages(params.page_size, total_items);

    let raid_quests = state.db.raid_quests.find_all(&params, &filters).await?;

    let response = PaginatedResponse {
        data: raid_quests,
        meta: PaginationMetadata {
            page: params.page,
            page_size: params.page_size,
            total_items,
            total_pages,
        },
    };

    Ok(Json(response))
}

/// Get Leaderboard for a specific Raid
pub async fn handle_get_raid_leaderboard(
    State(state): State<AppState>,
    Path(raid_id): Path<i32>,
    Query(params): Query<LeaderboardQueryParams>,
) -> Result<Json<PaginatedResponse<RaidLeaderboard>>, AppError> {
    validate_pagination_query(params.page, params.page_size)?;

    let total_items = state.db.raid_leaderboards.get_total_items(raid_id).await? as u32;
    let total_pages = calculate_total_pages(params.page_size, total_items);

    let offset = (params.page - 1) * params.page_size;
    let entries = state
        .db
        .raid_leaderboards
        .get_entries(raid_id, params.page_size as i64, offset as i64)
        .await?;

    let response = PaginatedResponse {
        data: entries,
        meta: PaginationMetadata {
            page: params.page,
            page_size: params.page_size,
            total_items: total_items as u32,
            total_pages,
        },
    };

    Ok(Json(response))
}

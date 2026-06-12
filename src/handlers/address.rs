use axum::{
    extract::{Query, State},
    Extension, Json,
};

use crate::{
    handlers::{
        calculate_total_pages, validate_pagination_query, ListQueryParams, PaginatedResponse, PaginationMetadata,
    },
    http_server::AppState,
    models::{
        address::{AddressFilter, AddressSortColumn, AddressWithOptInAndAssociations},
        admin::Admin,
    },
    AppError,
};

pub async fn handle_get_addresses(
    State(state): State<AppState>,
    Extension(_): Extension<Admin>,
    Query(params): Query<ListQueryParams<AddressSortColumn>>,
    Query(filters): Query<AddressFilter>,
) -> Result<Json<PaginatedResponse<AddressWithOptInAndAssociations>>, AppError> {
    validate_pagination_query(params.page, params.page_size)?;

    let total_items = state.db.addresses.count_filtered(&params, &filters).await? as u32;
    let total_pages = calculate_total_pages(params.page_size, total_items);

    let addresses = state
        .db
        .addresses
        .find_all_with_optin_and_associations(&params, &filters)
        .await?;

    let response = PaginatedResponse::<AddressWithOptInAndAssociations> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        models::admin::Admin,
        utils::{
            test_app_state::create_test_app_state,
            test_db::{create_persisted_address, create_persisted_eth_association, create_persisted_opt_in, reset_database},
        },
    };
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        routing::get,
        Extension, Router,
    };
    use tower::ServiceExt;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_handle_get_addresses_success() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        let addr1 = create_persisted_address(&state.db.addresses, "A1").await;
        let addr2 = create_persisted_address(&state.db.addresses, "A2").await;
        let addr3 = create_persisted_address(&state.db.addresses, "A3").await;

        create_persisted_opt_in(&state.db.pool, &addr1.quan_address.0).await;

        create_persisted_eth_association(
            &state.db.pool,
            &addr2.quan_address.0,
            "0x00000000219ab540356cBB839Cbe05303d7705Fa",
        )
        .await;

        let admin = Admin {
            id: Uuid::new_v4(),
            username: "new-user".to_string(),
            password: "what-ever".to_string(),
            updated_at: chrono::Utc::now(),
            created_at: chrono::Utc::now(),
        };

        let router = Router::new()
            .route("/", get(handle_get_addresses))
            .layer(Extension(admin))
            .with_state(state);

        let response = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/?page=1&page_size=10")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

        let meta = &body_json["meta"];
        assert_eq!(meta["total_items"], 3);
        assert_eq!(meta["page"], 1);

        let data = body_json["data"].as_array().unwrap();
        assert_eq!(data.len(), 3);

        let res_addr1 = data
            .iter()
            .find(|x| x["address"]["quan_address"] == addr1.quan_address.0)
            .unwrap();
        assert_eq!(res_addr1["is_opted_in"], true);
        assert!(!res_addr1["opt_in_number"].is_null());

        let res_addr2 = data
            .iter()
            .find(|x| x["address"]["quan_address"] == addr2.quan_address.0)
            .unwrap();
        assert_eq!(res_addr2["eth_address"], "0x00000000219ab540356cBB839Cbe05303d7705Fa");

        let res_addr3 = data
            .iter()
            .find(|x| x["address"]["quan_address"] == addr3.quan_address.0)
            .unwrap();
        assert_eq!(res_addr3["is_opted_in"], false);
        assert!(res_addr3["eth_address"].is_null());
    }
}

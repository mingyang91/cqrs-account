use crate::command_extractor::CommandExtractor;
use crate::state::ApplicationState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use cqrs_es::persist::ViewRepository;
use crate::account::commands::AccountCommand;
use crate::order::commands::OrderCommand;
use crate::transfer::commands::TransferCommand;

// Serves as our query endpoint to respond with the materialized `BankAccountView`
// for the requested account.
pub async fn account_query_handler(
    Path(account_id): Path<String>,
    State(state): State<ApplicationState>,
) -> Response {
    let view = match state.account_query.load(&account_id).await {
        Ok(view) => view,
        Err(err) => {
            tracing::error!("Error: {:#?}\n", err);
            return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
        }
    };
    match view {
        None => StatusCode::NOT_FOUND.into_response(),
        Some(account_view) => (StatusCode::OK, Json(account_view)).into_response(),
    }
}

// Serves as our command endpoint to make changes in a `BankAccount` aggregate.
pub async fn account_command_handler(
    Path(account_id): Path<String>,
    State(state): State<ApplicationState>,
    CommandExtractor(metadata, command): CommandExtractor<AccountCommand>,
) -> Response {
    match state
        .account_cqrs
        .execute_with_metadata(&account_id, command, metadata)
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(err) =>  {
            tracing::error!("Error: {:#?}\n", err);
            (StatusCode::BAD_REQUEST, err.to_string()).into_response()
        },
    }
}

pub async fn transfer_query_handler(
    Path(transfer_id): Path<String>,
    State(state): State<ApplicationState>,
) -> Response {
    let view = match state.transfer_query.load(&transfer_id).await {
        Ok(view) => view,
        Err(err) => {
            tracing::error!("Error: {:#?}\n", err);
            return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
        }
    };
    match view {
        None => StatusCode::NOT_FOUND.into_response(),
        Some(transfer_view) => (StatusCode::OK, Json(transfer_view)).into_response(),
    }
}

pub async fn transfer_command_handler(
    Path(transfer_id): Path<String>,
    State(state): State<ApplicationState>,
    CommandExtractor(metadata, command): CommandExtractor<TransferCommand>,
) -> Response {
    match state
        .transfer_cqrs
        .execute_with_metadata(&transfer_id, command, metadata)
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => {
            tracing::error!("Error: {:#?}\n", err);
            (StatusCode::BAD_REQUEST, err.to_string()).into_response()
        },
    }
}

pub async fn order_query_handler(
    Path(order_id): Path<String>,
    State(state): State<ApplicationState>,
) -> Response {
    let view = match state.order_query.load(&order_id).await {
        Ok(view) => view,
        Err(err) => {
            tracing::error!("Error: {:#?}\n", err);
            return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
        }
    };
    match view {
        None => StatusCode::NOT_FOUND.into_response(),
        Some(order_view) => (StatusCode::OK, Json(order_view)).into_response(),
    }
}

pub async fn order_command_handler(
    Path(order_id): Path<String>,
    State(state): State<ApplicationState>,
    CommandExtractor(metadata, command): CommandExtractor<OrderCommand>,
) -> Response {
    match state
        .order_cqrs
        .execute_with_metadata(&order_id, command, metadata)
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => {
            tracing::error!("Error: {:#?}\n", err);
            (StatusCode::BAD_REQUEST, err.to_string()).into_response()
        },
    }
}

use axum::routing::get;
use axum::Router;
use tokio::net::TcpListener;
use cqrs_account::route_handler::{
    account_command_handler,
    account_query_handler,
    transfer_query_handler,
    transfer_command_handler,
    order_query_handler,
    order_command_handler,
};
use cqrs_account::state::new_application_state;

#[tokio::main]
async fn main() {
    let connection_string = std::env::var("DATABASE_URL").unwrap_or("postgresql://postgres:postgres@postgres:5432/postgres".to_string());
    let state = new_application_state(&connection_string).await;
    // Configure the Axum routes and services.
    // For this example a single logical endpoint is used and the HTTP method
    // distinguishes whether the call is a command or a query.
    let router = Router::new()
        .route(
            "/account/:account_id",
            get(account_query_handler).post(account_command_handler),
        )
        .route("/transfer/:transfer_id", get(transfer_query_handler).post(transfer_command_handler))
        .route("/order/:order_id", get(order_query_handler).post(order_command_handler))
        .with_state(state);
    // Start the Axum server.
    let listen = TcpListener::bind("0.0.0.0:3030").await.expect("unable to bind TCP listener");
    axum::serve(listen, router.into_make_service())
        .await
        .unwrap();
}

use std::sync::Arc;

use cqrs_es::Query;
use postgres_es::{PostgresCqrs, PostgresViewRepository};
use sqlx::{Pool, Postgres};

use crate::account::aggregate::Account;
use crate::account::queries::{AccountQuery, AccountView};
use crate::order::aggregate::{Order, OrderServices};
use crate::order::queries::{OrderQuery, OrderView};
use crate::services::{BankAccountServices, HappyPathBankAccountServices};
use crate::transfer::aggregate::{Transfer, TransferServices};
use crate::transfer::queries::{TransferQuery, TransferView};

pub fn account_cqrs_framework(
    pool: Pool<Postgres>,
) -> (
    Arc<PostgresCqrs<Account>>,
    Arc<PostgresViewRepository<AccountView, Account>>,
) {
    // A very simple query that writes each event to stdout.
    let simple_query = crate::account::queries::SimpleLoggingQuery {};

    // A query that stores the current state of an individual account.
    let account_view_repo = Arc::new(PostgresViewRepository::new("account_query", pool.clone()));
    let mut account_query = AccountQuery::new(account_view_repo.clone());

    // Without a query error handler there will be no indication if an
    // error occurs (e.g., database connection failure, missing columns or table).
    // Consider logging an error or panicking in your own application.
    account_query.use_error_handler(Box::new(|e| println!("{}", e)));

    // Create and return an event-sourced `CqrsFramework`.
    let queries: Vec<Box<dyn Query<Account>>> =
        vec![Box::new(simple_query), Box::new(account_query)];
    let services = BankAccountServices::new(Box::new(HappyPathBankAccountServices));
    (
        Arc::new(postgres_es::postgres_snapshot_cqrs(
            pool, queries, 100, services,
        )),
        account_view_repo,
    )
}

pub fn transfer_cqrs_framework(pool: Pool<Postgres>, account_cqrs: Arc<PostgresCqrs<Account>>) -> (Arc<PostgresCqrs<Transfer>>, Arc<PostgresViewRepository<TransferView, Transfer>>) {
    let simple_query = crate::transfer::queries::SimpleLoggingQuery {};

    let transfer_view_repo = Arc::new(PostgresViewRepository::new("transfer_query", pool.clone()));
    let mut transfer_query = TransferQuery::new(transfer_view_repo.clone());
    transfer_query.use_error_handler(Box::new(|e| println!("{}", e)));

    let queries: Vec<Box<dyn Query<Transfer>>> = vec![Box::new(simple_query), Box::new(transfer_query)];
    let services = TransferServices::new(account_cqrs);

    (
        Arc::new(postgres_es::postgres_snapshot_cqrs(
            pool, queries, 100, services,
        )),
        transfer_view_repo,
    )
}

pub fn order_cqrs_framework(pool: Pool<Postgres>, account_cqrs: Arc<PostgresCqrs<Account>>) -> (Arc<PostgresCqrs<Order>>, Arc<PostgresViewRepository<OrderView, Order>>) {
    let simple_query = crate::order::queries::SimpleLoggingQuery {};

    let order_view_repo = Arc::new(PostgresViewRepository::new("order_query", pool.clone()));
    let mut order_query = OrderQuery::new(order_view_repo.clone());
    order_query.use_error_handler(Box::new(|e| println!("{}", e)));

    let queries: Vec<Box<dyn Query<Order>>> = vec![Box::new(simple_query), Box::new(order_query)];
    let services = OrderServices::new(account_cqrs);

    (
        Arc::new(postgres_es::postgres_snapshot_cqrs(
            pool, queries, 100, services,
        )),
        order_view_repo,
    )
}
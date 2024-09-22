use crate::account::aggregate::Account;
use crate::config::{account_cqrs_framework, transfer_cqrs_framework};
use postgres_es::{default_postgress_pool, PostgresCqrs, PostgresViewRepository};
use std::sync::Arc;
use crate::account::queries::BankAccountView;
use crate::transfer::aggregate::Transfer;
use crate::transfer::queries::TransferView;

#[derive(Clone)]
pub struct ApplicationState {
    pub account_cqrs: Arc<PostgresCqrs<Account>>,
    pub account_query: Arc<PostgresViewRepository<BankAccountView, Account>>,
    pub transfer_cqrs: Arc<PostgresCqrs<Transfer>>,
    pub transfer_query: Arc<PostgresViewRepository<TransferView, Transfer>>,
}

pub async fn new_application_state(connection_string: &str) -> ApplicationState {
    // Configure the CQRS framework, backed by a Postgres database, along with two queries:
    // - a simply-query prints events to stdout as they are published
    // - `account_query` stores the current state of the account in a ViewRepository that we can access
    //
    // The needed database tables are automatically configured with `docker-compose up -d`,
    // see init file at `/db/init.sql` for more.
    let pool = default_postgress_pool(connection_string).await;
    let (account_cqrs, account_query) = account_cqrs_framework(pool.clone());
    let (transfer_cqrs, transfer_query) = transfer_cqrs_framework(pool, account_cqrs.clone());
    ApplicationState {
        account_cqrs,
        account_query,
        transfer_cqrs,
        transfer_query,
    }
}

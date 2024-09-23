use async_trait::async_trait;
use cqrs_es::persist::GenericQuery;
use cqrs_es::{EventEnvelope, Query, View};
use postgres_es::PostgresViewRepository;
use serde::{Deserialize, Serialize};
use crate::util::types::ByteArray32;
use super::aggregate::Transfer;
use super::events::TransferEvent;

pub struct SimpleLoggingQuery {}

// Our simplest query, this is great for debugging but absolutely useless in production.
// This query just pretty prints the events as they are processed.
#[async_trait]
impl Query<Transfer> for SimpleLoggingQuery {
    async fn dispatch(&self, aggregate_id: &str, events: &[EventEnvelope<Transfer>]) {
        for event in events {
            let payload = serde_json::to_string_pretty(&event.payload).unwrap();
            println!("{}-{}\n{}", aggregate_id, event.sequence, payload);
        }
    }
}

// Our second query, this one will be handled with Postgres `GenericQuery`
// which will serialize and persist our view after it is updated. It also
// provides a `load` method to deserialize the view on request.
pub type TransferQuery = GenericQuery<
    PostgresViewRepository<TransferView, Transfer>,
    TransferView,
    Transfer,
>;

// The view for a Transfer query, for a standard http application this should
// be designed to reflect the response dto that will be returned to a user.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TransferView {
    transfer_id: Option<ByteArray32>,
    from_account: String,
    to_account: String,
    amount: u64,
    asset: String,
    create_timestamp: u64,
    update_timestamp: u64,
    description: String,
    is_done: bool,
    failed_reason: Option<String>,
}

// This updates the view with events as they are committed.
// The logic should be minimal here, e.g., don't calculate the account balance,
// design the events to carry the balance information instead.
impl View<Transfer> for TransferView {
    fn update(&mut self, event: &EventEnvelope<Transfer>) {
        match &event.payload {
            TransferEvent::Opened { transfer_id, from_account, to_account, amount, asset, timestamp, description } => {
                self.transfer_id = Some(*transfer_id);
                self.from_account = from_account.clone();
                self.to_account = to_account.clone();
                self.amount = *amount;
                self.asset = asset.clone();
                self.create_timestamp = *timestamp;
                self.description = description.clone();
                self.is_done = false;
            }
            TransferEvent::Done { timestamp } => {
                self.update_timestamp = *timestamp;
                self.is_done = true;
            },
            TransferEvent::Failed { reason, timestamp } => {
                self.update_timestamp = *timestamp;
                self.failed_reason = Some(reason.clone())
            }
        }
    }
}

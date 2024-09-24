use async_trait::async_trait;
use cqrs_es::{EventEnvelope, Query, View};
use cqrs_es::persist::GenericQuery;
use postgres_es::PostgresViewRepository;
use serde::{Deserialize, Serialize};
use crate::order::aggregate::Order;
use crate::order::events::OrderEvent;

pub struct SimpleLoggingQuery {}

#[derive(Debug, Serialize, Deserialize, Default)]
pub enum OrderState {
    #[default]
    Initial,
    Placed,
    Cancelling,
    Cancelled,
    Buying,
    Bought,
    Failed,
    Settled,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct OrderView {
    pub id: String,
    pub buyer: Option<String>,
    pub seller: String,
    pub sell_asset: String,
    pub sell_amount: u64,
    pub buy_asset: String,
    pub buy_amount: u64,
    pub status: OrderState,
    pub reason: Option<String>,
    pub create_time: u64,
    pub update_time: u64,
    pub settle_time: Option<u64>,
}

#[async_trait]
impl Query<Order> for SimpleLoggingQuery {
    async fn dispatch(&self, aggregate_id: &str, events: &[EventEnvelope<Order>]) {
        for event in events {
            let payload = serde_json::to_string_pretty(&event.payload).unwrap();
            tracing::debug!("{}-{}\n{}", aggregate_id, event.sequence, payload);
        }
    }
}

pub type OrderQuery = GenericQuery<
    PostgresViewRepository<OrderView, Order>,
    OrderView,
    Order,
>;

impl View<Order> for OrderView {
    fn update(&mut self, event: &EventEnvelope<Order>) {
        match &event.payload {
            OrderEvent::Initialized { config } => {
                self.id = config.order_id.hex();
                self.seller = config.seller.clone();
                self.sell_asset = config.sell_asset.clone();
                self.sell_amount = config.sell_amount;
                self.buy_asset = config.buy_asset.clone();
                self.buy_amount = config.buy_amount;
                self.status = OrderState::Initial;
                self.create_time = config.timestamp;
                self.update_time = config.timestamp;

            }
            OrderEvent::Placed { timestamp } => {
                self.update_time = *timestamp;
                self.status = OrderState::Placed;
            }
            OrderEvent::Cancelling { timestamp, reason } => {
                self.update_time = *timestamp;
                self.reason = Some(reason.clone());
                self.status = OrderState::Cancelling;
            }
            OrderEvent::Cancelled { timestamp } => {
                self.update_time = *timestamp;
                self.status = OrderState::Cancelled;
            }
            OrderEvent::Buying { buyer, timestamp } => {
                self.buyer = Some(buyer.clone());
                self.update_time = *timestamp;
                self.status = OrderState::Buying;
            }
            OrderEvent::Bought { timestamp } => {
                self.update_time = *timestamp;
                self.status = OrderState::Bought;
            }
            OrderEvent::Failed { timestamp, reason } => {
                self.update_time = *timestamp;
                self.reason = Some(reason.clone());
                self.status = OrderState::Failed;
            }
            OrderEvent::Settled { timestamp } => {
                self.update_time = *timestamp;
                self.settle_time = Some(*timestamp);
                self.status = OrderState::Settled;
            }
        }
    }
}
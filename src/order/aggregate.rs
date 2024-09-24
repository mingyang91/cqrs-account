use std::mem::swap;
use std::sync::Arc;
use async_trait::async_trait;
use cqrs_es::{Aggregate, AggregateError};
use futures::future::BoxFuture;
use postgres_es::PostgresCqrs;
use serde::{Deserialize, Serialize};
use crate::account::aggregate::Account;
use crate::account::commands::AccountCommand;
use crate::account::events::AccountError;
use crate::order::commands::OrderCommand;
use crate::order::events::{OrderConfig, OrderEvent};
use crate::util::transaction_guard::TransactionGuard;
use crate::util::types::ByteArray32;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum Order {
    #[default]
    Uninitialized,
    Initialized {
        config: OrderConfig,
    },
    Placed {
        config: OrderConfig,
        timestamp: u64,
    },
    Cancelling {
        config: OrderConfig,
        reason: String,
        timestamp: u64,
    },
    Cancelled {
        config: OrderConfig,
        timestamp: u64,
        reason: String,
    },
    Buying {
        config: OrderConfig,
        buyer: String,
        timestamp: u64,
    },
    Bought {
        config: OrderConfig,
        buyer: String,
        timestamp: u64,
    },
    Failed {
        config: OrderConfig,
        timestamp: u64,
        reason: String,
    },
    Settled {
        config: OrderConfig,
        timestamp: u64,
    },
}

impl Order {
    fn id(&self) -> Option<ByteArray32> {
        match self {
            Order::Uninitialized => None,
            Order::Initialized { config } => Some(config.order_id),
            Order::Placed { config, .. } => Some(config.order_id),
            Order::Cancelling { config, .. } => Some(config.order_id),
            Order::Cancelled { config, .. } => Some(config.order_id),
            Order::Buying { config, .. } => Some(config.order_id),
            Order::Bought { config, .. } => Some(config.order_id),
            Order::Failed { config, .. } => Some(config.order_id),
            Order::Settled { config, .. } => Some(config.order_id),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OrderError {
    #[error("Invalid state: {0}")]
    InvalidState(String),
    #[error("Account error: {0}")]
    AccountError(#[from] AccountError),
    #[error("Aggregate error: {0}")]
    AggregateError(#[from] AggregateError<AccountError>),
}

#[derive(Clone)]
pub struct OrderServices {
    account_service: Arc<PostgresCqrs<Account>>,
}

impl OrderServices {
    pub fn new(account_service: Arc<PostgresCqrs<Account>>) -> Self {
        OrderServices { account_service }
    }

    async fn lock_funds(
        &self,
        order_id: ByteArray32,
        seller: String,
        sell_asset: String,
        sell_amount: u64,
        timestamp: u64,
    ) -> Result<TransactionGuard<BoxFuture<'static, ()>>, OrderError> {
        let account_service = self.account_service.clone();
        let undo = {
            let account_service = account_service.clone();
            let seller = seller.clone();
            async move {
                tracing::info!("Undo: unlock funds for {} in order {}", seller, order_id.hex());
                let command = AccountCommand::unlock_funds(order_id);
                match account_service.execute(&seller, command).await {
                    Ok(_) | Err(AggregateError::UserError(AccountError::LockNotFound)) => {}
                    Err(e) => {
                        tracing::error!("Failed to unlock funds: {:?}", e);
                    }
                }
            }
        };
        let command = AccountCommand::lock_funds(
            order_id,
            timestamp,
            sell_asset.clone(),
            sell_amount,
        );
        match self.account_service.execute(&seller, command).await {
            Ok(_) | Err(AggregateError::UserError(AccountError::DuplicateLock)) => {
                Ok(TransactionGuard::new(Box::pin(undo)))
            },
            Err(AggregateError::UserError(ae)) => {
                undo.await;
                Err(OrderError::AccountError(ae))
            },
            Err(e) => {
                undo.await;
                tracing::error!("Failed to lock funds due to framework error: {:?}", e);
                Err(OrderError::AggregateError(e))
            },
        }
    }

    async fn unlock_funds(
        &self,
        order_id: ByteArray32,
        seller: String,
    ) -> Result<(), OrderError> {
        let command = AccountCommand::unlock_funds(order_id);
        match self.account_service.execute(&seller, command).await {
            Ok(_) => Ok(()),
            Err(AggregateError::UserError(ae)) => {
                Err(OrderError::AccountError(ae))
            },
            Err(e) => Err(OrderError::AggregateError(e)),
        }
    }

    async fn settle(
        &self,
        order_id: ByteArray32,
        account_id: String,
        pair_account_id: String,
        receive_asset: String,
        receive_amount: u64,
    ) -> Result<(), OrderError> {
        let command = AccountCommand::settle(
            order_id,
            pair_account_id,
            receive_asset,
            receive_amount,
        );
        match self.account_service.execute(&account_id, command).await {
            Ok(_) | Err(AggregateError::UserError(AccountError::DuplicateTransaction(_))) => Ok(()),
            Err(AggregateError::UserError(ae)) => {
                Err(OrderError::AccountError(ae))
            },
            Err(e) => Err(OrderError::AggregateError(e)),
        }
    }
}

#[async_trait]
impl Aggregate for Order {
    type Command = OrderCommand;
    type Event = OrderEvent;
    type Error = OrderError;
    type Services = OrderServices;

    fn aggregate_type() -> String {
        "order".to_string()
    }

    async fn handle(
        &self,
        command: Self::Command,
        services: &Self::Services,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        let span = tracing::span!(tracing::Level::INFO, "Order::handle", order_id = self.id().map(|id| id.hex()));
        let _ = span.enter();
        match (self, command) {
            (Order::Uninitialized, OrderCommand::Open { config }) => {
                let event = OrderEvent::Initialized { config };
                Ok(vec![event])
            },
            (Order::Initialized { config }, OrderCommand::Continue) => {
                let now = chrono::Utc::now().timestamp() as u64;
                match services.lock_funds(
                    config.order_id,
                    config.seller.clone(),
                    config.sell_asset.clone(),
                    config.sell_amount,
                    now,
                ).await {
                    Err(OrderError::AccountError(ae)) => {
                        Ok(vec![OrderEvent::Failed {
                            timestamp: now,
                            reason: format!("Failed to lock funds: {:?}", ae),
                        }])
                    },
                    Err(e) => Err(e),
                    Ok(lock_undo) => {
                        let event = OrderEvent::Placed {
                            timestamp: now,
                        };
                        lock_undo.commit();
                        Ok(vec![event])
                    },
                }
            },
            (Order::Placed { .. }, OrderCommand::Cancel { reason }) => {
                let event = OrderEvent::Cancelling {
                    timestamp: chrono::Utc::now().timestamp() as u64,
                    reason,
                };
                Ok(vec![event])
            },
            (Order::Cancelling { config, timestamp, .. }, OrderCommand::Continue) => {
                services.unlock_funds(config.order_id, config.seller.clone()).await?;
                let event = OrderEvent::Cancelled {
                    timestamp: *timestamp,
                };
                Ok(vec![event])
            },
            (Order::Placed { .. }, OrderCommand::Buy { buyer, timestamp }) => {
                let event = OrderEvent::Buying {
                    buyer,
                    timestamp,
                };
                Ok(vec![event])
            },
            (Order::Buying { config, buyer, timestamp }, OrderCommand::Continue) => {
                match services.lock_funds(
                    config.order_id,
                    buyer.clone(),
                    config.buy_asset.clone(),
                    config.buy_amount,
                    *timestamp
                ).await {
                    Err(OrderError::AccountError(ae)) => {
                        tracing::info!("Failed to lock funds: {:?}", ae);
                        Ok(vec![OrderEvent::Placed {
                            timestamp: *timestamp
                        }])
                    },
                    Err(e) => Err(e),
                    Ok(lock_undo) => {
                        let event = OrderEvent::Bought {
                            timestamp: chrono::Utc::now().timestamp() as u64,
                        };
                        lock_undo.commit();
                        Ok(vec![event])
                    },
                }
            },
            (Order::Bought { config, buyer, timestamp }, OrderCommand::Continue) => {
                services.settle(
                    config.order_id,
                    config.seller.clone(),
                    buyer.clone(),
                    config.buy_asset.clone(),
                    config.buy_amount
                ).await?;
                services.settle(
                    config.order_id,
                    buyer.clone(),
                    config.seller.clone(),
                    config.sell_asset.clone(),
                    config.sell_amount
                ).await?;
                let event = OrderEvent::Settled {
                    timestamp: *timestamp,
                };
                Ok(vec![event])
            },
            (state, cmd) => {
                Err(OrderError::InvalidState(format!("Order current at {:?} state, cannot accept {:?} command", state, cmd)))
            }
        }
    }

    fn apply(&mut self, event: Self::Event) {
        let mut prev = Default::default();
        swap(&mut prev, self);
        match (prev, event) {
            (Order::Uninitialized, OrderEvent::Initialized { config }) => {
                *self = Order::Initialized {
                    config,
                };
            },
            (Order::Initialized { ref mut config }, OrderEvent::Placed { timestamp }) => {
                let mut temp = Default::default();
                swap(&mut temp, config);
                *self = Order::Placed {
                    config: temp,
                    timestamp,
                };
            },
            (Order::Placed { ref mut config, .. }, OrderEvent::Cancelling { timestamp, reason }) => {
                let mut temp = Default::default();
                swap(&mut temp, config);
                *self = Order::Cancelling {
                    config: temp,
                    timestamp,
                    reason
                };
            },
            (Order::Cancelling { ref mut config, reason, .. }, OrderEvent::Cancelled { timestamp }) => {
                let mut temp = Default::default();
                swap(&mut temp, config);
                *self = Order::Cancelled {
                    config: temp,
                    timestamp,
                    reason: reason.clone()
                };
            },
            (Order::Placed { ref mut config, .. }, OrderEvent::Buying { buyer, timestamp }) => {
                let mut temp = Default::default();
                swap(&mut temp, config);
                *self = Order::Buying {
                    config: temp,
                    buyer,
                    timestamp,
                };
            },
            (Order::Buying { ref mut config, ref mut buyer, .. }, OrderEvent::Bought { timestamp }) => {
                let mut temp = Default::default();
                swap(&mut temp, config);
                let mut temp_buyer = Default::default();
                swap(&mut temp_buyer, buyer);
                *self = Order::Bought {
                    config: temp,
                    timestamp,
                    buyer: temp_buyer
                };
            },
            (Order::Buying { ref mut config, .. }, OrderEvent::Failed { timestamp, reason }) => {
                let mut temp = Default::default();
                swap(&mut temp, config);
                *self = Order::Failed {
                    config: temp,
                    timestamp,
                    reason,
                };
            },
            (Order::Bought { ref mut config, .. }, OrderEvent::Settled { timestamp }) => {
                let mut temp = Default::default();
                swap(&mut temp, config);
                *self = Order::Settled {
                    config: temp,
                    timestamp,
                };
            },
            (Order::Buying { ref mut config, .. }, OrderEvent::Placed { timestamp }) => {
                let mut temp = Default::default();
                swap(&mut temp, config);
                *self = Order::Placed {
                    config: temp,
                    timestamp,
                };
            },
            (state, event) => unreachable!("Invalid state transition: {:?} -> {:?}", state, event),
        }
    }
}
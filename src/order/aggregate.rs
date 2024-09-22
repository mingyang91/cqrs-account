use std::sync::Arc;
use cqrs_es::AggregateError;
use futures::future::BoxFuture;
use postgres_es::PostgresCqrs;
use serde::{Deserialize, Serialize};
use crate::account::aggregate::Account;
use crate::account::commands::AccountCommand;
use crate::account::events::AccountError;
use crate::order::events::OrderConfig;
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
            let order_id = order_id.clone();
            let seller = seller.clone();
            async move {
                let command = AccountCommand::funds_unlocked(order_id);
                match account_service.execute(&seller, command).await {
                    Ok(_) | Err(AggregateError::UserError(AccountError::LockNotFound)) => {}
                    Err(e) => {
                        tracing::error!("Failed to unlock funds: {:?}", e);
                    }
                }
            }
        };
        let command = AccountCommand::funds_locked(
            order_id.clone(),
            timestamp,
            sell_asset.clone(),
            sell_amount,
        );
        match self.account_service.execute(&seller, command).await {
            Ok(_) | Err(AggregateError::UserError(AccountError::DuplicateTransaction(_))) => {
                Ok(TransactionGuard::new(Box::pin(undo)))
            },
            Err(e) => {
                undo.await;
                Err(OrderError::AggregateError(e))
            },
        }
    }
}

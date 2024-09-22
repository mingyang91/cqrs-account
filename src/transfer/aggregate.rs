#![deny(arithmetic_overflow)]

use std::mem::swap;
use futures::future::BoxFuture;
use std::sync::Arc;

use async_trait::async_trait;
use cqrs_es::{Aggregate, AggregateError};
use postgres_es::PostgresCqrs;
use serde::{Deserialize, Serialize};

use crate::{
    account::{
        aggregate::BankAccount,
        commands::{BankAccountCommand, ByteArray32},
        events::BankAccountError,
    },
    util::transaction_guard::TransactionGuard,
};

use super::{commands::TransferCommand, events::TransferEvent};

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Config {
    pub transfer_id: ByteArray32,
    pub from_account: String,
    pub to_account: String,
    pub asset: String,
    pub amount: u64,
    pub timestamp: u64,
    pub description: String,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub enum Transfer {
    #[default]
    Uninitialized,
    Opened {
        config: Config,
    },
    Done {
        config: Config,
        timestamp: u64,
    },
    Failed {
        config: Config,
        reason: String,
        timestamp: u64,
    },
    Canceled {
        config: Config,
        reason: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum TransferError {
    #[error("Invalid state: {0}")]
    InvalidState(String),
    #[error("Bank account error: {0}")]
    AccountError(#[from] BankAccountError),
    #[error("Aggregate error: {0}")]
    AggregateError(#[from] AggregateError<BankAccountError>),
}

#[derive(Clone)]
pub struct TransferServices {
    account_service: Arc<PostgresCqrs<BankAccount>>,
}

impl TransferServices {
    pub fn new(account_service: Arc<PostgresCqrs<BankAccount>>) -> Self {
        Self { account_service }
    }

    async fn debit(
        &self,
        txid: ByteArray32,
        from_account: String,
        to_account: String,
        asset: String,
        amount: u64,
        timestamp: u64,
    ) -> Result<TransactionGuard<BoxFuture<'static, ()>>, TransferError> {
        let account_service = self.account_service.clone();
        let undo = {
            let from_account = from_account.clone();
            let to_account = to_account.clone();
            let asset = asset.clone();
            let amount = amount;
            async move {
                let command =
                    BankAccountCommand::reverse_debit(txid, timestamp, to_account.clone(), asset, amount);
                match account_service.execute(&from_account, command).await {
                    Ok(_) => {}
                    Err(AggregateError::UserError(BankAccountError::TransactionNotFound)) => {}
                    Err(e) => {
                        tracing::error!("Error undoing debit: {:?}", e);
                    }
                }
            }
        };

        let command = BankAccountCommand::debit(txid, timestamp, to_account, asset, amount);

        match self.account_service.execute(&from_account, command).await {
            Ok(_) => Ok(TransactionGuard::new(Box::pin(undo))),
            Err(AggregateError::UserError(BankAccountError::DuplicateTransaction(_))) => {
                Ok(TransactionGuard::new(Box::pin(undo)))
            }
            Err(agg_err) => {
                undo.await;
                Err(TransferError::AggregateError(agg_err))
            }
        }
    }

    async fn credit(
        &self,
        txid: ByteArray32,
        from_account: String,
        to_account: String,
        asset: String,
        amount: u64,
        timestamp: u64,
    ) -> Result<TransactionGuard<BoxFuture<'static, ()>>, TransferError> {
        let account_service = self.account_service.clone();
        let undo = {
            let from_account = from_account.clone();
            let to_account = to_account.clone();
            let asset = asset.clone();
            let amount = amount;
            async move {
                let command = BankAccountCommand::reverse_credit(
                    txid,
                    timestamp,
                    from_account,
                    asset,
                    amount,
                );

                match account_service.execute(&to_account, command).await {
                    Ok(_) => {}
                    Err(AggregateError::UserError(BankAccountError::TransactionNotFound)) => {}
                    Err(e) => {
                        tracing::error!("Error undoing credit: {:?}", e);
                    }
                }
            }
        };

        let command = BankAccountCommand::credit(txid, timestamp, from_account, asset, amount);

        match self.account_service.execute(&to_account, command).await {
            Ok(_) => Ok(TransactionGuard::new(Box::pin(undo))),
            Err(AggregateError::UserError(BankAccountError::DuplicateTransaction(_))) => {
                Ok(TransactionGuard::new(Box::pin(undo)))
            }
            Err(agg_err) => {
                undo.await;
                Err(TransferError::AggregateError(agg_err))
            }
        }
    }
}

#[async_trait]
impl Aggregate for Transfer {
    type Command = TransferCommand;
    type Event = TransferEvent;
    type Error = TransferError;
    type Services = TransferServices;

    fn aggregate_type() -> String {
        "transfer".to_string()
    }

    async fn handle(
        &self,
        command: Self::Command,
        service: &Self::Services,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        match command {
            TransferCommand::Open {
                transfer_id,
                from_account,
                to_account,
                asset,
                amount,
                timestamp,
                description,
            } => {
                if let Transfer::Uninitialized = self {
                    Ok(vec![TransferEvent::Opened {
                        transfer_id,
                        from_account,
                        to_account,
                        asset,
                        amount,
                        timestamp,
                        description,
                    }])
                } else {
                    Err(TransferError::InvalidState(
                        "Transfer is already opened".to_string(),
                    ))
                }
            },
            TransferCommand::Continue => {
                let Transfer::Opened { config } = self else {
                    return Err(TransferError::InvalidState(
                        "State is not Opened".to_string(),
                    ));
                };
                let timestamp = 0;
                let debit_undo_guard = service
                    .debit(
                        config.transfer_id,
                        config.from_account.to_string(),
                        config.to_account.to_string(),
                        config.asset.to_string(),
                        config.amount,
                        timestamp,
                    )
                    .await?;
                let credit_undo_guard = service
                    .credit(
                        config.transfer_id,
                        config.from_account.to_string(),
                        config.to_account.to_string(),
                        config.asset.to_string(),
                        config.amount,
                        timestamp,
                    )
                    .await?;
                credit_undo_guard.commit();
                debit_undo_guard.commit();
                Ok(vec![TransferEvent::Done { timestamp }])
            }
        }
    }

    fn apply(&mut self, event: Self::Event) {
        match event {
            TransferEvent::Opened {
                transfer_id,
                from_account,
                to_account,
                asset,
                amount,
                timestamp,
                description,
            } => {
                *self = Transfer::Opened {
                    config: Config {
                        transfer_id,
                        from_account,
                        to_account,
                        asset,
                        amount,
                        timestamp,
                        description,
                    },
                }
            }
            TransferEvent::Failed { reason, timestamp } => {
                let mut temp = Default::default();
                if let Transfer::Opened { config } = self {
                    swap(&mut temp, config);
                }
                *self = Transfer::Failed {
                    config: temp,
                    reason,
                    timestamp
                }
            }
            TransferEvent::Done { timestamp } => {
                let mut temp = Default::default();
                if let Transfer::Opened { config } = self {
                    swap(&mut temp, config);
                }
                *self = Transfer::Done {
                    config: temp,
                    timestamp
                }
            }
        }
    }
}

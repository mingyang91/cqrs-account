#![deny(arithmetic_overflow)]
use std::{error::Error, future::Future, pin::Pin, sync::Arc};

use async_trait::async_trait;
use cqrs_es::{Aggregate, AggregateError};
use postgres_es::PostgresCqrs;
use serde::{Deserialize, Serialize};

use crate::account::{
    aggregate::BankAccount,
    commands::{BankAccountCommand, ByteArray32},
    events::BankAccountError,
};

use super::{commands::TransferCommand, events::TransferEvent};

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Config {
    pub transfer_id: String,
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
    },
    Failed {
        config: Config,
    },
    Canceled {
        config: Config,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum TransferError {
    #[error("Invalid state: {0}")]
    InvalidState(String),
    #[error("Bank account error: {0}")]
    AccountError(#[from] BankAccountError),
    #[error("Aggregate error: {0}")]
    InternalError(#[from] Box<dyn Error>),
}

pub struct TransferServices {
    account_service: Arc<PostgresCqrs<BankAccount>>,
}

#[async_trait]
impl Aggregate for Transfer {
    type Event = TransferEvent;
    type Command = TransferCommand;
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
            } => Ok(vec![TransferEvent::Opened {
                transfer_id,
                from_account,
                to_account,
                asset,
                amount,
                timestamp,
                description,
            }]),
            TransferCommand::Continue => {
                let Transfer::Opened { config } = self else {
                    return Err(TransferError::InvalidState(
                        "State is not Opened".to_string(),
                    ));
                };
                let timestamp = 0;
                if let Err(e) = service
                    .account_service
                    .execute(
                        &config.from_account,
                        BankAccountCommand::debited(
                            ByteArray32([0; 32]),
                            timestamp,
                            config.to_account.to_string(),
                            config.asset.to_string(),
                            config.amount,
                        ),
                    )
                    .await
                {
                    return if let AggregateError::UserError(ae) = e {
                        Err(TransferError::AccountError(ae))
                    } else {
                        Err(TransferError::InternalError(Box::new(e)))
                    };
                }
                if let Err(e) = service
                    .account_service
                    .execute(
                        &config.to_account,
                        BankAccountCommand::credited(
                            ByteArray32([0; 32]),
                            timestamp,
                            config.from_account.to_string(),
                            config.asset.to_string(),
                            config.amount,
                        ),
                    )
                    .await
                {
                    return if let AggregateError::UserError(ae) = e {
                        Err(TransferError::AccountError(ae))
                    } else {
                        Err(TransferError::InternalError(Box::new(e)))
                    };
                }
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
                todo!()
            }
            TransferEvent::Done { timestamp } => {
                todo!()
            }
        }
    }
}

struct TransactionGuard<Fut>
where
    Fut: core::future::Future<Output = ()> + Send + Sync + 'static,
{
    redo: Option<Fut>,
}

impl<Fut> Drop for TransactionGuard<Fut>
where
    Fut: Future<Output = ()> + Send + Sync + 'static,
{
    fn drop(&mut self) {
        if let Some(redo) = self.redo.take() {
            tokio::spawn(redo);
        }
    }
}

impl<Fut> TransactionGuard<Fut>
where
    Fut: Future<Output = ()> + Send + Sync + 'static,
{
    pub fn new(redo: Fut) -> Self {
        Self { redo: Some(redo) }
    }

    pub fn commit(mut self) {
        self.redo = None;
    }
}

use cqrs_es::Aggregate;
use serde::{Deserialize, Serialize};
use async_trait::async_trait;

use super::{commands::TransferCommand, events::TransferEvent};

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct TransferInvoice {
    pub transfer_id: String,
    pub from_account: String,
    pub to_account: String,
    pub asset: String,
    pub amount: u64,
    pub timestamp: u64,
    pub description: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum TransferState {
    Started,
    FromAccountDebited,
    ToAccountCredited,
    Settled, // <- Success & End
    // Failure
    FromAccountCreditFailed(String), // <- Failure & Undo
    ToAccountCreditFailed(String), // <- Failure & Undo
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub enum Transfer {
    #[default]
    Uninitialized,
    Opened {
        invoice: TransferInvoice,
        state: TransferState,
    }
}

#[derive(Serialize, Deserialize, Debug, thiserror::Error)]
pub enum TransferError {
}

pub struct TransferServices {
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
    
    async fn handle(&self, command: Self::Command, service: &Self::Services) -> Result<Vec<Self::Event>, Self::Error> {
        match command {
            TransferCommand::Open { transfer_id, from_account, to_account, asset, amount, timestamp, description } => {
                Ok(vec![TransferEvent::Opened { transfer_id, from_account, to_account, asset, amount, timestamp, description }])
            },
            TransferCommand::Cancel { reason, timestamp } => {
                Ok(vec![TransferEvent::Canceled { reason, timestamp }])
            },
            TransferCommand::Retry { timestamp } => {
                Ok(vec![TransferEvent::Retried { timestamp }])
            },
        }
    }
    
    fn apply(&mut self, event: Self::Event) {
        match event {
            TransferEvent::Opened { transfer_id, from_account, to_account, asset, amount, timestamp, description } => {
                *self = Transfer::Opened {
                    invoice: TransferInvoice { transfer_id, from_account, to_account, asset, amount, timestamp, description },
                    state: TransferState::Started,
                }
            },
            TransferEvent::Canceled { reason, timestamp } => {
                *self = Transfer::Uninitialized
            },
            TransferEvent::Retried { timestamp } => {
                todo!()
            },
        }
    }
}

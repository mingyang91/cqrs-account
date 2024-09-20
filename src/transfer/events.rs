use cqrs_es::DomainEvent;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransferEvent {
    Opened {
        transfer_id: String,
        from_account: String,
        to_account: String,
        asset: String,
        amount: u64,
        timestamp: u64,
        description: String,
    },
    Done {
        timestamp: u64,
    },
    Failed {
        reason: String,
        timestamp: u64,
    },
}

impl DomainEvent for TransferEvent {
    fn event_type(&self) -> String {
        match self {
            TransferEvent::Opened { .. } => "Opened".to_string(),
            TransferEvent::Done { .. } => "Done".to_string(),
            TransferEvent::Failed { .. } => "Failed".to_string(),
        }
    }

    fn event_version(&self) -> String {
        "1.0".to_string()
    }
}

use cqrs_es::DomainEvent;
use serde::{Deserialize, Serialize};
use crate::util::types::ByteArray32;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct OrderConfig {
    pub order_id: ByteArray32,
    pub seller: String,
    pub sell_asset: String,
    pub sell_amount: u64,
    pub buy_asset: String,
    pub buy_amount: u64,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrderEvent {
    Initialized {
        config: OrderConfig,
    },
    Placed {
        timestamp: u64,
    },
    Cancelling {
        timestamp: u64,
        reason: String,
    },
    Cancelled {
        timestamp: u64,
    },
    Buying {
        buyer: String,
        timestamp: u64
    },
    Bought {
        timestamp: u64,
    },
    Failed {
        timestamp: u64,
        reason: String,
    },
    Settled {
        timestamp: u64,
    },
}

impl DomainEvent for OrderEvent {
    fn event_type(&self) -> String {
        match self {
            OrderEvent::Initialized { .. } => "Initialized".to_string(),
            OrderEvent::Placed { .. } => "Placed".to_string(),
            OrderEvent::Cancelling { .. } => "Cancelling".to_string(),
            OrderEvent::Cancelled { .. } => "Cancelled".to_string(),
            OrderEvent::Buying { .. } => "Buying".to_string(),
            OrderEvent::Bought { .. } => "Bought".to_string(),
            OrderEvent::Failed { .. } => "Failed".to_string(),
            OrderEvent::Settled { .. } => "Settled".to_string(),
        }
    }

    fn event_version(&self) -> String {
        "1.0".to_string()
    }
}
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderConfig {
    pub order_id: String,
    pub seller: String,
    pub sell_asset: String,
    pub sell_amount: u64,
    pub buy_asset: String,
    pub buy_amount: u64,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderEvent {
    Initialized {
        config: OrderConfig,
    },
    Placed {
        timestamp: u64,
    },
    Cancelled {
        timestamp: u64,
        reason: String,
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
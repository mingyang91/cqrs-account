use serde::{Deserialize, Serialize};
use crate::order::events::OrderConfig;

#[derive(Debug, Serialize, Deserialize)]
pub enum OrderCommand {
    Open {
        config: OrderConfig
    },
    Continue,
    Cancel {
        reason: String,
    },
    Buy {
        buyer: String,
        timestamp: u64,
    },
}
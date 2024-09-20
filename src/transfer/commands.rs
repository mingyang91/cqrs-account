use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum TransferCommand {
    Open {
        transfer_id: String,
        from_account: String,
        to_account: String,
        asset: String,
        amount: u64,
        timestamp: u64,
        description: String,
    },
    Continue,
}

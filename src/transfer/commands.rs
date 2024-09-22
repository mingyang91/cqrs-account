use serde::{Deserialize, Serialize};
use crate::util::types::ByteArray32;

#[derive(Debug, Serialize, Deserialize)]
pub enum TransferCommand {
    Open {
        transfer_id: ByteArray32,
        from_account: String,
        to_account: String,
        asset: String,
        amount: u64,
        timestamp: u64,
        description: String,
    },
    Continue,
}

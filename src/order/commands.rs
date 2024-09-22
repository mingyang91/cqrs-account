use crate::util::types::ByteArray32;

pub enum OrderCommand {
    Open {
        order_id: ByteArray32,
        seller: String,
        sell_asset: String,
        sell_amount: u64,
        buy_asset: String,
        buy_amount: u64,
        timestamp: u64,
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
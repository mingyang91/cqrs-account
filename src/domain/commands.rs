use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord, Clone, Copy)]
#[serde(transparent)]
pub struct ByteArray32(pub [u8; 32]);

#[derive(Debug, Serialize, Deserialize)]
pub enum BankAccountCommand {
    Account(AccountCommand),
    Transaction { 
        timestamp: u64,
        txid: ByteArray32,
        command: TransactionCommand 
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum AccountCommand {
    Open { account_id: String },
    Disable,
    Enable,
    Close,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum TransactionCommand {
    Deposit { asset: String, amount: u64 },
    Withdraw { asset: String, amount: u64 },
    LockFunds { order_id: ByteArray32, asset: String, amount: u64, expiration: u64 }, // into Reserving
    UnlockFunds { order_id: ByteArray32 }, // cancel Reserving
    ExpirationUnlockFunds { order_id: ByteArray32 }, // into Unlocked
    Settle { order_id: ByteArray32, to_account: String },
    PartialSettle { order_id: ByteArray32, to_account: String, amount: u64 },
}

impl BankAccountCommand {
    pub fn account_opened(account_id: String) -> Self {
        BankAccountCommand::Account(AccountCommand::Open { account_id })
    }

    pub fn account_disabled() -> Self {
        BankAccountCommand::Account(AccountCommand::Disable)
    }

    pub fn account_enabled() -> Self {
        BankAccountCommand::Account(AccountCommand::Enable)
    }

    pub fn account_closed() -> Self {
        BankAccountCommand::Account(AccountCommand::Close)
    }

    pub fn deposited(txid: ByteArray32, timestamp: u64, asset: String, amount: u64) -> Self {
        BankAccountCommand::Transaction {
            timestamp,
            txid,
            command: TransactionCommand::Deposit { asset, amount },
        }
    }

    pub fn withdrew(txid: ByteArray32, timestamp: u64, asset: String, amount: u64) -> Self {
        BankAccountCommand::Transaction {
            timestamp,
            txid,
            command: TransactionCommand::Withdraw { asset, amount },
        }
    }

    pub fn funds_locked(
        txid: ByteArray32,
        timestamp: u64,
        order_id: ByteArray32,
        asset: String,
        amount: u64,
        expiration: u64,
    ) -> Self {
        BankAccountCommand::Transaction {
            timestamp,
            txid,
            command: TransactionCommand::LockFunds {
                order_id,
                asset,
                amount,
                expiration,
            },
        }
    }

    pub fn funds_unlocked(txid: ByteArray32, order_id: ByteArray32) -> Self {
        BankAccountCommand::Transaction {
            timestamp: 0,
            txid,
            command: TransactionCommand::UnlockFunds { order_id },
        }
    }

    pub fn expiration_unlocked(txid: ByteArray32, order_id: ByteArray32) -> Self {
        BankAccountCommand::Transaction {
            timestamp: 0,
            txid,
            command: TransactionCommand::ExpirationUnlockFunds { order_id },
        }
    }

    pub fn settled(txid: ByteArray32, order_id: ByteArray32, to_account: String) -> Self {
        BankAccountCommand::Transaction {
            timestamp: 0,
            txid,
            command: TransactionCommand::Settle { order_id, to_account },
        }
    }

    pub fn partial_settled(txid: ByteArray32, order_id: ByteArray32, to_account: String, amount: u64) -> Self {
        BankAccountCommand::Transaction {
            timestamp: 0,
            txid,
            command: TransactionCommand::PartialSettle { order_id, to_account, amount },
        }
    }
}
use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt::Write;
use crate::util::types::ByteArray32;

#[derive(Debug, Serialize, Deserialize)]
pub enum AccountCommand {
    Lifecycle(LifecycleCommand),
    Transaction {
        timestamp: u64,
        txid: ByteArray32,
        command: TransactionCommand,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum LifecycleCommand {
    Open { account_id: String },
    Disable,
    Enable,
    Close,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum TransactionCommand {
    Deposit {
        asset: String,
        amount: u64,
    },
    Withdraw {
        asset: String,
        amount: u64,
    },
    Debit {
        to_account: String,
        asset: String,
        amount: u64,
    },
    ReverseDebit {
        to_account: String,
        asset: String,
        amount: u64,
    },
    Credit {
        from_account: String,
        asset: String,
        amount: u64,
    },
    ReverseCredit {
        from_account: String,
        asset: String,
        amount: u64,
    },
    LockFunds {
        asset: String,
        amount: u64,
    }, // into Reserving
    UnlockFunds, // cancel Reserving
    Settle {
        to_account: String,
    },
}

impl AccountCommand {
    pub fn account_opened(account_id: String) -> Self {
        AccountCommand::Lifecycle(LifecycleCommand::Open { account_id })
    }

    pub fn account_disabled() -> Self {
        AccountCommand::Lifecycle(LifecycleCommand::Disable)
    }

    pub fn account_enabled() -> Self {
        AccountCommand::Lifecycle(LifecycleCommand::Enable)
    }

    pub fn account_closed() -> Self {
        AccountCommand::Lifecycle(LifecycleCommand::Close)
    }

    pub fn deposited(txid: ByteArray32, timestamp: u64, asset: String, amount: u64) -> Self {
        AccountCommand::Transaction {
            timestamp,
            txid,
            command: TransactionCommand::Deposit { asset, amount },
        }
    }

    pub fn withdrew(txid: ByteArray32, timestamp: u64, asset: String, amount: u64) -> Self {
        AccountCommand::Transaction {
            timestamp,
            txid,
            command: TransactionCommand::Withdraw { asset, amount },
        }
    }

    pub fn debit(
        txid: ByteArray32,
        timestamp: u64,
        to_account: String,
        asset: String,
        amount: u64,
    ) -> Self {
        AccountCommand::Transaction {
            timestamp,
            txid,
            command: TransactionCommand::Debit {
                to_account,
                asset,
                amount,
            },
        }
    }

    pub fn reverse_debit(
        txid: ByteArray32,
        timestamp: u64,
        to_account: String,
        asset: String,
        amount: u64,
    ) -> Self {
        AccountCommand::Transaction {
            timestamp,
            txid,
            command: TransactionCommand::ReverseDebit {
                to_account,
                asset,
                amount,
            },
        }
    }

    pub fn credit(
        txid: ByteArray32,
        timestamp: u64,
        from_account: String,
        asset: String,
        amount: u64,
    ) -> Self {
        AccountCommand::Transaction {
            timestamp,
            txid,
            command: TransactionCommand::Credit {
                from_account,
                asset,
                amount,
            },
        }
    }

    pub fn reverse_credit(
        txid: ByteArray32,
        timestamp: u64,
        from_account: String,
        asset: String,
        amount: u64,
    ) -> Self {
        AccountCommand::Transaction {
            timestamp,
            txid,
            command: TransactionCommand::ReverseCredit {
                from_account,
                asset,
                amount,
            },
        }
    }

    pub fn lock_funds(
        txid: ByteArray32,
        timestamp: u64,
        asset: String,
        amount: u64,
    ) -> Self {
        AccountCommand::Transaction {
            timestamp,
            txid,
            command: TransactionCommand::LockFunds {
                asset,
                amount,
            },
        }
    }

    pub fn unlock_funds(txid: ByteArray32) -> Self {
        AccountCommand::Transaction {
            timestamp: 0,
            txid,
            command: TransactionCommand::UnlockFunds,
        }
    }

    pub fn settle(txid: ByteArray32, to_account: String) -> Self {
        AccountCommand::Transaction {
            timestamp: 0,
            txid,
            command: TransactionCommand::Settle {
                to_account,
            },
        }
    }
}

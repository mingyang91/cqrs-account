use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord, Clone, Copy)]
#[serde(transparent)]
pub struct ByteArray32(pub [u8; 32]);

impl ByteArray32 {
    pub fn hex(&self) -> String {
        hex::encode(self.0)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum BankAccountCommand {
    Account(AccountCommand),
    Transaction {
        timestamp: u64,
        txid: ByteArray32,
        command: TransactionCommand,
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
        from_account: String,
        asset: String,
        amount: u64,
    },
    Credit {
        from_account: String,
        asset: String,
        amount: u64,
    },
    ReverseCredit {
        to_account: String,
        asset: String,
        amount: u64,
    },
    LockFunds {
        order_id: ByteArray32,
        asset: String,
        amount: u64,
        expiration: u64,
    }, // into Reserving
    UnlockFunds {
        order_id: ByteArray32,
    }, // cancel Reserving
    Settle {
        order_id: ByteArray32,
        to_account: String,
    },
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

    pub fn debit(
        txid: ByteArray32,
        timestamp: u64,
        to_account: String,
        asset: String,
        amount: u64,
    ) -> Self {
        BankAccountCommand::Transaction {
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
        from_account: String,
        asset: String,
        amount: u64,
    ) -> Self {
        BankAccountCommand::Transaction {
            timestamp,
            txid,
            command: TransactionCommand::ReverseDebit {
                from_account,
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
        BankAccountCommand::Transaction {
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
        to_account: String,
        asset: String,
        amount: u64,
    ) -> Self {
        BankAccountCommand::Transaction {
            timestamp,
            txid,
            command: TransactionCommand::ReverseCredit {
                to_account,
                asset,
                amount,
            },
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

    pub fn settled(txid: ByteArray32, order_id: ByteArray32, to_account: String) -> Self {
        BankAccountCommand::Transaction {
            timestamp: 0,
            txid,
            command: TransactionCommand::Settle {
                order_id,
                to_account,
            },
        }
    }
}

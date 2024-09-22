use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt::Write;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord, Clone, Copy, Default)]
#[serde(transparent)]
pub struct ByteArray32(pub [u8; 32]);

// impl <'de> Deserialize<'de> for ByteArray32 {
//     fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
//     where
//         D: Deserializer<'de>
//     {
//         // 1. hex str
//         // 2. base64 str
//         // 3. base58 str
//         // 4. number array
//
//         // visitor
//         struct ByteArray32Visitor;
//
//         impl<'de> Visitor<'de> for ByteArray32Visitor {
//             type Value = ByteArray32;
//
//             fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
//                 formatter.write_str("a 32-byte array")
//             }
//
//             fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
//             where
//                 E: de::Error,
//             {
//                 if value.len() == 64 {
//                     let mut bytes = [0u8; 32];
//                     hex::decode_to_slice(value, &mut bytes).map_err(de::Error::custom)?;
//                     Ok(ByteArray32(bytes))
//                 } else {
//                     Err(de::Error::custom("invalid length"))
//                 }
//             }
//
//             fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E>
//             where
//                 E: de::Error,
//             {
//                 if value.len() == 32 {
//                     let mut bytes = [0u8; 32];
//                     bytes.copy_from_slice(value);
//                     Ok(ByteArray32(bytes))
//                 } else {
//                     Err(de::Error::custom("invalid length"))
//                 }
//             }
//         }
//
//         deserializer.deserialize_str(ByteArray32Visitor)
//     }
// }
//
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
        to_account: String,
        asset: String,
        amount: u64,
    ) -> Self {
        BankAccountCommand::Transaction {
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
        from_account: String,
        asset: String,
        amount: u64,
    ) -> Self {
        BankAccountCommand::Transaction {
            timestamp,
            txid,
            command: TransactionCommand::ReverseCredit {
                from_account,
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

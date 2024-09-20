use std::collections::{BTreeMap, VecDeque};

use async_trait::async_trait;
use cqrs_es::persist::GenericQuery;
use cqrs_es::{EventEnvelope, Query, View};
use postgres_es::PostgresViewRepository;
use serde::{Deserialize, Serialize};

use crate::account::aggregate::BankAccount;
use crate::account::events::{AccountEvent, BankAccountEvent, TransactionEvent};

const RECENT_LEDGER_SIZE: usize = 100;

pub struct SimpleLoggingQuery {}

// Our simplest query, this is great for debugging but absolutely useless in production.
// This query just pretty prints the events as they are processed.
#[async_trait]
impl Query<BankAccount> for SimpleLoggingQuery {
    async fn dispatch(&self, aggregate_id: &str, events: &[EventEnvelope<BankAccount>]) {
        for event in events {
            let payload = serde_json::to_string_pretty(&event.payload).unwrap();
            println!("{}-{}\n{}", aggregate_id, event.sequence, payload);
        }
    }
}

// Our second query, this one will be handled with Postgres `GenericQuery`
// which will serialize and persist our view after it is updated. It also
// provides a `load` method to deserialize the view on request.
pub type AccountQuery = GenericQuery<
    PostgresViewRepository<BankAccountView, BankAccount>,
    BankAccountView,
    BankAccount,
>;

// The view for a BankAccount query, for a standard http application this should
// be designed to reflect the response dto that will be returned to a user.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct BankAccountView {
    account_id: Option<String>,
    is_disabled: bool,
    balance: BTreeMap<String, u64>,
    locked_balance: BTreeMap<String, u64>,
    recent_ledger: VecDeque<LedgerEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LedgerEntry {
    timestamp: u64,
    txid: String,
    detail: LedgerDetail,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "@t")]
pub enum LedgerDetail {
    Deposit {
        asset: String,
        amount: u64,
    },
    Withdraw {
        asset: String,
        amount: u64,
    },
    Debited {
        to_account: String,
        asset: String,
        amount: u64,
    },
    Credited {
        from_account: String,
        asset: String,
        amount: u64,
    },
    Lock {
        asset: String,
        amount: u64,
    },
    Unlock {
        asset: String,
        amount: u64,
    },
    ExpireUnlock {
        asset: String,
        amount: u64,
    },
    Settlement {
        to_account: String,
        amount: u64,
    },
    PartialSettlement {
        to_account: String,
        amount: u64,
    },
}

// This updates the view with events as they are committed.
// The logic should be minimal here, e.g., don't calculate the account balance,
// design the events to carry the balance information instead.
impl View<BankAccount> for BankAccountView {
    fn update(&mut self, event: &EventEnvelope<BankAccount>) {
        match &event.payload {
            BankAccountEvent::Account(account_event) => match account_event {
                AccountEvent::AccountOpened { account_id } => {
                    self.account_id = Some(account_id.clone());
                }
                AccountEvent::AccountClosed => {
                    *self = Default::default();
                }
                AccountEvent::AccountDisabled => {
                    self.is_disabled = true;
                }
                AccountEvent::AccountEnabled => {
                    self.is_disabled = false;
                }
            },
            BankAccountEvent::Transaction {
                timestamp,
                txid,
                event,
            } => match event {
                TransactionEvent::Deposited { asset, amount } => {
                    self.balance
                        .entry(asset.clone())
                        .and_modify(|e| *e += *amount)
                        .or_insert(*amount);
                    self.recent_ledger.push_front(LedgerEntry {
                        timestamp: *timestamp,
                        txid: txid.hex(),
                        detail: LedgerDetail::Deposit {
                            asset: asset.clone(),
                            amount: *amount,
                        },
                    });
                }
                TransactionEvent::Withdrew { asset, amount } => {
                    self.balance
                        .entry(asset.clone())
                        .and_modify(|e| *e -= *amount)
                        .or_insert(0);
                    self.recent_ledger.push_front(LedgerEntry {
                        timestamp: *timestamp,
                        txid: txid.hex(),
                        detail: LedgerDetail::Withdraw {
                            asset: asset.clone(),
                            amount: *amount,
                        },
                    });
                }
                TransactionEvent::Debited {
                    to_account,
                    asset,
                    amount,
                } => {
                    self.balance
                        .entry(asset.clone())
                        .and_modify(|e| *e -= *amount)
                        .or_insert(0);
                    self.recent_ledger.push_front(LedgerEntry {
                        timestamp: *timestamp,
                        txid: txid.hex(),
                        detail: LedgerDetail::Debited {
                            to_account: to_account.clone(),
                            asset: asset.clone(),
                            amount: *amount,
                        },
                    });
                }
                TransactionEvent::Credited {
                    from_account,
                    asset,
                    amount,
                } => {
                    self.balance
                        .entry(asset.clone())
                        .and_modify(|e| *e += amount)
                        .or_insert(*amount);
                    self.recent_ledger.push_front(LedgerEntry {
                        timestamp: *timestamp,
                        txid: txid.hex(),
                        detail: LedgerDetail::Credited {
                            from_account: from_account.clone(),
                            asset: asset.clone(),
                            amount: *amount,
                        },
                    });
                }
                TransactionEvent::FundsLocked {
                    order_id,
                    asset,
                    amount,
                    expiration,
                } => {
                    self.balance
                        .entry(asset.clone())
                        .and_modify(|e| *e -= *amount)
                        .or_insert(0);
                    self.locked_balance
                        .entry(asset.clone())
                        .and_modify(|e| *e += *amount)
                        .or_insert(*amount);
                    self.recent_ledger.push_front(LedgerEntry {
                        timestamp: *timestamp,
                        txid: txid.hex(),
                        detail: LedgerDetail::Lock {
                            asset: asset.clone(),
                            amount: *amount,
                        },
                    });
                }
                TransactionEvent::FundsUnlocked { order_id } => {
                    todo!()
                }
                TransactionEvent::Settled {
                    order_id,
                    to_account,
                } => todo!(),
            },
        }
    }
}

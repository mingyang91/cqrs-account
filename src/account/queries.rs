use std::collections::{BTreeMap, VecDeque};

use async_trait::async_trait;
use cqrs_es::persist::GenericQuery;
use cqrs_es::{EventEnvelope, Query, View};
use postgres_es::PostgresViewRepository;
use serde::{Deserialize, Serialize};
use crate::account::aggregate::Account;
use crate::account::events::{LifecycleEvent, AccountEvent, TransactionEvent};

const RECENT_LEDGER_SIZE: usize = 100;

pub struct SimpleLoggingQuery {}

// Our simplest query, this is great for debugging but absolutely useless in production.
// This query just pretty prints the events as they are processed.
#[async_trait]
impl Query<Account> for SimpleLoggingQuery {
    async fn dispatch(&self, aggregate_id: &str, events: &[EventEnvelope<Account>]) {
        for event in events {
            let payload = serde_json::to_string_pretty(&event.payload).unwrap();
            tracing::debug!("{}-{}\n{}", aggregate_id, event.sequence, payload);
        }
    }
}

// Our second query, this one will be handled with Postgres `GenericQuery`
// which will serialize and persist our view after it is updated. It also
// provides a `load` method to deserialize the view on request.
pub type AccountQuery = GenericQuery<
    PostgresViewRepository<AccountView, Account>,
    AccountView,
    Account,
>;

// The view for a BankAccount query, for a standard http application this should
// be designed to reflect the response dto that will be returned to a user.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AccountView {
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
    DebitReversed {
        to_account: String,
        asset: String,
        amount: u64,
    },
    Credited {
        from_account: String,
        asset: String,
        amount: u64,
    },
    CreditReversed {
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
    Settlement {
        to_account: String,
        send_asset: String,
        send_amount: u64,
        receive_asset: String,
        receive_amount: u64
    },
}

impl AccountView {
    fn add_ledger(&mut self, entry: LedgerEntry) {
        self.recent_ledger.push_front(entry);
        if self.recent_ledger.len() > RECENT_LEDGER_SIZE {
            self.recent_ledger.pop_back();
        }
    }
}

// This updates the view with events as they are committed.
// The logic should be minimal here, e.g., don't calculate the account balance,
// design the events to carry the balance information instead.
impl View<Account> for AccountView {
    fn update(&mut self, event: &EventEnvelope<Account>) {
        match &event.payload {
            AccountEvent::Lifecycle(account_event) => match account_event {
                LifecycleEvent::Opened { account_id } => {
                    self.account_id = Some(account_id.clone());
                }
                LifecycleEvent::Closed => {
                    *self = Default::default();
                }
                LifecycleEvent::Disabled => {
                    self.is_disabled = true;
                }
                LifecycleEvent::Enabled => {
                    self.is_disabled = false;
                }
            },
            AccountEvent::Transaction {
                timestamp,
                txid,
                event,
            } => match event {
                TransactionEvent::Deposited { asset, amount } => {
                    self.balance
                        .entry(asset.clone())
                        .and_modify(|e| *e += *amount)
                        .or_insert(*amount);
                    self.add_ledger(LedgerEntry {
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
                    self.add_ledger(LedgerEntry {
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
                    self.add_ledger(LedgerEntry {
                        timestamp: *timestamp,
                        txid: txid.hex(),
                        detail: LedgerDetail::Debited {
                            to_account: to_account.clone(),
                            asset: asset.clone(),
                            amount: *amount,
                        },
                    });
                }
                TransactionEvent::DebitReversed {
                    to_account,
                    asset,
                    amount,
                } => {
                    self.balance
                        .entry(asset.clone())
                        .and_modify(|e| *e += *amount)
                        .or_insert(*amount);
                    self.add_ledger(LedgerEntry {
                        timestamp: *timestamp,
                        txid: txid.hex(),
                        detail: LedgerDetail::DebitReversed {
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
                    self.add_ledger(LedgerEntry {
                        timestamp: *timestamp,
                        txid: txid.hex(),
                        detail: LedgerDetail::Credited {
                            from_account: from_account.clone(),
                            asset: asset.clone(),
                            amount: *amount,
                        },
                    });
                }
                TransactionEvent::CreditReversed {
                    from_account,
                    asset,
                    amount,
                } => {
                    self.balance
                        .entry(asset.clone())
                        .and_modify(|e| *e -= *amount)
                        .or_insert(0);
                    self.add_ledger(LedgerEntry {
                        timestamp: *timestamp,
                        txid: txid.hex(),
                        detail: LedgerDetail::CreditReversed {
                            from_account: from_account.clone(),
                            asset: asset.clone(),
                            amount: *amount,
                        },
                    });
                }
                TransactionEvent::FundsLocked {
                    asset,
                    amount,
                } => {
                    self.balance
                        .entry(asset.clone())
                        .and_modify(|e| *e -= *amount)
                        .or_insert_with(|| unreachable!("asset not found due to lock, it should not happens"));
                    self.locked_balance
                        .entry(asset.clone())
                        .and_modify(|e| *e += *amount)
                        .or_insert(*amount);
                    self.add_ledger(LedgerEntry {
                        timestamp: *timestamp,
                        txid: txid.hex(),
                        detail: LedgerDetail::Lock {
                            asset: asset.clone(),
                            amount: *amount,
                        },
                    });
                }
                TransactionEvent::FundsUnlocked { asset, amount } => {
                    self.balance
                        .entry(asset.clone())
                        .and_modify(|e| *e += *amount)
                        .or_insert(*amount);
                    self.locked_balance
                        .entry(asset.clone())
                        .and_modify(|e| *e -= *amount)
                        .or_insert_with(|| unreachable!("asset not exists due to unlock, it should not happens"));
                    self.add_ledger(LedgerEntry {
                        timestamp: *timestamp,
                        txid: txid.hex(),
                        detail: LedgerDetail::Unlock {
                            asset: asset.clone(),
                            amount: *amount,
                        },
                    });
                }
                TransactionEvent::Settled {
                    to_account,
                    send_asset,
                    send_amount,
                    receive_asset,
                    receive_amount,
                } => {
                    self.locked_balance
                        .entry(send_asset.clone())
                        .and_modify(|e| {
                            e.checked_sub(*send_amount)
                                .unwrap_or_else(|| panic!("account: [{}] lock {} {} in order, but {} will be withdrew!", self.account_id.to_owned().unwrap_or("???".to_string()), e, send_asset, send_amount));
                        })
                        .or_insert_with(|| unreachable!("locked asset not exists, it should not happens"));
                    self.balance
                        .entry(receive_asset.clone())
                        .and_modify(|e| *e += *receive_amount)
                        .or_insert(*receive_amount);
                    self.add_ledger(LedgerEntry {
                        timestamp: *timestamp,
                        txid: txid.hex(),
                        detail: LedgerDetail::Settlement {
                            to_account: to_account.clone(),
                            send_asset: send_asset.clone(),
                            send_amount: *send_amount,
                            receive_asset: receive_asset.clone(),
                            receive_amount: *receive_amount,
                        },
                    });
                }
            },
        }
    }
}

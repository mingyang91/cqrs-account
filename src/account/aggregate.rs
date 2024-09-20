#![deny(arithmetic_overflow)]
use std::collections::{BTreeMap, VecDeque};
use std::mem;

use async_trait::async_trait;
use cqrs_es::Aggregate;
use serde::{Deserialize, Serialize};

use crate::account::commands::BankAccountCommand;
use crate::account::events::{BankAccountError, BankAccountEvent};
use crate::services::BankAccountServices;

use super::commands::{AccountCommand, ByteArray32, TransactionCommand};
use super::events::{AccountEvent, TransactionEvent};

const DEFAULT_TTL: u64 = 30 * 24 * 60 * 60;

#[derive(Serialize, Deserialize, Default)]
struct ProcessedTransactions {
    ttl: u64,
    txids: BTreeMap<ByteArray32, u64>,
    timeseries: VecDeque<(u64, ByteArray32)>,
}

impl ProcessedTransactions {
    fn new(ttl: u64) -> Self {
        Self {
            ttl,
            txids: BTreeMap::new(),
            timeseries: VecDeque::new(),
        }
    }

    fn get_timestamp(&self, txid: &ByteArray32) -> Option<u64> {
        self.txids.get(txid).copied()
    }

    fn insert(&mut self, txid: ByteArray32, timestamp: u64) -> Result<(), u64> {
        if let Some(txts) = self.txids.get(&txid) {
            return Err(*txts);
        }

        self.txids.insert(txid, timestamp);
        self.timeseries.push_back((timestamp, txid));

        while let Some((txts, txid)) = self.timeseries.pop_front() {
            if txts + self.ttl < timestamp {
                self.txids.remove(&txid);
            } else {
                self.timeseries.push_front((timestamp, txid));
                break;
            }
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct ReservedFunds {
    asset: String,
    amount: u64,
    expiration: u64,
}

#[derive(Serialize, Deserialize, Default)]
pub enum BankAccount {
    #[default]
    Uninitialized,
    InService {
        state: BankAccountState,
    },
    Disabled {
        state: BankAccountState,
    },
    Closed,
}

#[derive(Serialize, Deserialize, Default)]
pub struct BankAccountState {
    account_id: String,
    assets: BTreeMap<String, u64>,
    reserving: BTreeMap<ByteArray32, ReservedFunds>,
    processed_transactions: ProcessedTransactions,
}

impl BankAccountState {
    fn is_empty(&self) -> bool {
        self.assets.is_empty() && self.reserving.is_empty()
    }
}

#[async_trait]
impl Aggregate for BankAccount {
    type Command = BankAccountCommand;
    type Event = BankAccountEvent;
    type Error = BankAccountError;
    type Services = BankAccountServices;

    // This identifier should be unique to the system.
    fn aggregate_type() -> String {
        "account".to_string()
    }

    // The aggregate logic goes here. Note that this will be the _bulk_ of a CQRS system
    // so expect to use helper functions elsewhere to keep the code clean.
    async fn handle(
        &self,
        command: Self::Command,
        services: &Self::Services,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        match command {
            BankAccountCommand::Account(command) => match command {
                AccountCommand::Open { account_id } => match self {
                    BankAccount::Uninitialized | BankAccount::Closed => {
                        Ok(vec![BankAccountEvent::account_opened(account_id)])
                    }
                    _ => Err(BankAccountError::AccountAlreadyExists),
                },
                AccountCommand::Disable => {
                    if let BankAccount::InService { .. } = self {
                        Ok(vec![BankAccountEvent::account_disabled()])
                    } else {
                        Err(BankAccountError::AccountNotInService)
                    }
                }
                AccountCommand::Enable => {
                    if let BankAccount::Disabled { .. } = self {
                        Ok(vec![BankAccountEvent::account_enabled()])
                    } else {
                        Err(BankAccountError::AccountNotDisabled)
                    }
                }
                AccountCommand::Close => match self {
                    BankAccount::Uninitialized | BankAccount::Closed => {
                        Err(BankAccountError::AccountNotFound)
                    }
                    BankAccount::InService { state } => {
                        if state.is_empty() {
                            Ok(vec![BankAccountEvent::account_closed()])
                        } else {
                            Err(BankAccountError::AccountNotEmpty)
                        }
                    }
                    BankAccount::Disabled { state } => {
                        if state.is_empty() {
                            Ok(vec![BankAccountEvent::account_closed()])
                        } else {
                            Err(BankAccountError::AccountNotEmpty)
                        }
                    }
                },
            },
            BankAccountCommand::Transaction {
                txid,
                timestamp,
                command,
            } => match self {
                BankAccount::Uninitialized | BankAccount::Closed => {
                    Err(BankAccountError::AccountNotFound)
                }
                BankAccount::Disabled { .. } => Err(BankAccountError::AccountNotInService),
                BankAccount::InService { state } => {
                    if let Some(processed) = state.processed_transactions.get_timestamp(&txid) {
                        return Err(BankAccountError::DuplicateTransaction(processed));
                    }
                    match command {
                        TransactionCommand::Deposit { asset, amount } => {
                            if let Some(timestamp) =
                                state.processed_transactions.get_timestamp(&txid)
                            {
                                return Err(BankAccountError::DuplicateTransaction(timestamp));
                            }
                            Ok(vec![BankAccountEvent::deposited(
                                txid, timestamp, asset, amount,
                            )])
                        }
                        TransactionCommand::Withdraw { asset, amount } => {
                            if let Some(timestamp) =
                                state.processed_transactions.get_timestamp(&txid)
                            {
                                return Err(BankAccountError::DuplicateTransaction(timestamp));
                            }
                            if state.assets.get(&asset).unwrap_or(&0) < &amount {
                                return Err(BankAccountError::InsufficientFunds);
                            }

                            Ok(vec![BankAccountEvent::withdrew(
                                txid, timestamp, asset, amount,
                            )])
                        }
                        TransactionCommand::Credit {
                            from_account,
                            asset,
                            amount,
                        } => {
                            if let Some(timestamp) =
                                state.processed_transactions.get_timestamp(&txid)
                            {
                                return Err(BankAccountError::DuplicateTransaction(timestamp));
                            }
                            Ok(vec![BankAccountEvent::credited(
                                txid,
                                timestamp,
                                from_account,
                                asset,
                                amount,
                            )])
                        }
                        TransactionCommand::ReverseCredit {
                            to_account,
                            asset,
                            amount,
                        } => todo!(),
                        TransactionCommand::ReverseDebit {
                            from_account,
                            asset,
                            amount,
                        } => todo!(),
                        TransactionCommand::Debit {
                            to_account,
                            asset,
                            amount,
                        } => {
                            if let Some(timestamp) =
                                state.processed_transactions.get_timestamp(&txid)
                            {
                                return Err(BankAccountError::DuplicateTransaction(timestamp));
                            }
                            if state.assets.get(&asset).unwrap_or(&0) < &amount {
                                return Err(BankAccountError::InsufficientFunds);
                            }

                            Ok(vec![BankAccountEvent::debited(
                                txid, timestamp, to_account, asset, amount,
                            )])
                        }
                        TransactionCommand::LockFunds {
                            order_id,
                            asset,
                            amount,
                            expiration,
                        } => {
                            if let Some(timestamp) =
                                state.processed_transactions.get_timestamp(&txid)
                            {
                                return Err(BankAccountError::DuplicateTransaction(timestamp));
                            }
                            if state.assets.get(&asset).unwrap_or(&0) < &amount {
                                return Err(BankAccountError::InsufficientFunds);
                            }

                            Ok(vec![BankAccountEvent::funds_locked(
                                txid, timestamp, order_id, asset, amount, expiration,
                            )])
                        }
                        TransactionCommand::UnlockFunds { order_id } => {
                            if state.reserving.contains_key(&txid) {
                                Ok(vec![BankAccountEvent::funds_unlocked(
                                    txid, timestamp, order_id,
                                )])
                            } else {
                                Err(BankAccountError::LockNotFound)
                            }
                        }
                        TransactionCommand::Settle {
                            order_id,
                            to_account,
                        } => {
                            if let Some(timestamp) =
                                state.processed_transactions.get_timestamp(&txid)
                            {
                                return Err(BankAccountError::DuplicateTransaction(timestamp));
                            }
                            Ok(vec![BankAccountEvent::settlement(
                                txid, timestamp, order_id, to_account,
                            )])
                        }
                    }
                }
            },
        }
    }

    fn apply(&mut self, event: Self::Event) {
        match event {
            BankAccountEvent::Account(account_event) => match account_event {
                AccountEvent::AccountOpened { account_id } => {
                    *self = BankAccount::InService {
                        state: BankAccountState {
                            account_id,
                            assets: BTreeMap::new(),
                            reserving: BTreeMap::new(),
                            processed_transactions: ProcessedTransactions::new(DEFAULT_TTL),
                        },
                    };
                }
                AccountEvent::AccountDisabled => {
                    let BankAccount::InService { state } = self else {
                        unreachable!("account should be in service");
                    };
                    let mut temp = BankAccountState::default();
                    mem::swap(state, &mut temp);
                    *self = BankAccount::Disabled { state: temp };
                }
                AccountEvent::AccountEnabled => {
                    let BankAccount::Disabled { state } = self else {
                        unreachable!("account should be disabled");
                    };
                    let mut temp = BankAccountState::default();
                    mem::swap(state, &mut temp);
                    *self = BankAccount::InService { state: temp };
                }
                AccountEvent::AccountClosed => {
                    *self = BankAccount::Closed;
                }
            },
            BankAccountEvent::Transaction {
                timestamp,
                txid,
                event,
            } => {
                let BankAccount::InService { ref mut state } = self else {
                    unreachable!("account should be in service");
                };

                state
                    .processed_transactions
                    .insert(txid, timestamp)
                    .expect("txid already processed, that should not happen");

                match event {
                    TransactionEvent::Deposited { asset, amount } => {
                        let balance = state.assets.entry(asset.to_owned()).or_insert(0);
                        *balance = balance
                            .checked_add(amount)
                            .expect("balance should not overflow");
                    }
                    TransactionEvent::Withdrew { asset, amount } => {
                        let balance = state.assets.entry(asset.to_owned()).or_insert(0);
                        *balance = balance
                            .checked_sub(amount)
                            .expect("balance should not be negative");
                    }
                    TransactionEvent::Debited { asset, amount, .. } => {
                        let balance = state.assets.entry(asset.to_owned()).or_insert(0);
                        *balance = balance
                            .checked_sub(amount)
                            .expect("balance should not be negative");
                    }
                    TransactionEvent::Credited { asset, amount, .. } => {
                        let balance = state.assets.entry(asset.to_owned()).or_insert(0);
                        *balance = balance
                            .checked_add(amount)
                            .expect("balance should not overflow");
                    }
                    TransactionEvent::FundsLocked {
                        order_id,
                        asset,
                        amount,
                        expiration,
                    } => {
                        let balance = state.assets.entry(asset.to_owned()).or_insert(0);
                        *balance = balance
                            .checked_sub(amount)
                            .expect("balance should not be negative");

                        state.reserving.insert(
                            order_id,
                            ReservedFunds {
                                asset,
                                amount,
                                expiration,
                            },
                        );
                    }
                    TransactionEvent::FundsUnlocked { order_id } => {
                        let reserved = state
                            .reserving
                            .remove(&order_id)
                            .expect("txid not found in reserving");
                        let balance = state.assets.entry(reserved.asset).or_insert(0);
                        *balance = balance
                            .checked_add(reserved.amount)
                            .expect("balance should not overflow");
                    }
                    TransactionEvent::Settlement {
                        order_id,
                        to_account: _,
                    } => {
                        state
                            .reserving
                            .remove(&order_id)
                            .expect("txid not found in reserving");
                    }
                }
            }
        }
    }
}

// The aggregate tests are the most important part of a CQRS system.
// The simplicity and flexibility of these tests are a good part of what
// makes an event sourced system so friendly to changing business requirements.
#[cfg(test)]
mod aggregate_tests {
    use async_trait::async_trait;
    use std::sync::Mutex;

    use cqrs_es::test::TestFramework;

    use crate::account::aggregate::BankAccount;
    use crate::account::commands::{BankAccountCommand, ByteArray32, TransactionCommand};
    use crate::account::events::BankAccountEvent;
    use crate::services::{AtmError, BankAccountApi, BankAccountServices, CheckingError};

    // A test framework that will apply our events and command
    // and verify that the logic works as expected.
    type AccountTestFramework = TestFramework<BankAccount>;

    #[test]
    fn test_deposit_money() {
        let expected =
            BankAccountEvent::deposited(ByteArray32([0; 32]), 0, "Satoshi".to_string(), 1000);
        let command =
            BankAccountCommand::deposited(ByteArray32([0; 32]), 0, "Satoshi".to_string(), 1000);

        let services = BankAccountServices::new(Box::new(MockBankAccountServices::default()));
        // Obtain a new test framework
        AccountTestFramework::with(services)
            // In a test case with no previous events
            .given_no_previous_events()
            // Wnen we fire this command
            .when(command)
            // then we expect these results
            .then_expect_events(vec![expected]);
    }

    #[test]
    fn test_deposit_money_with_balance() {
        let previous =
            BankAccountEvent::deposited(ByteArray32([0; 32]), 0, "Satoshi".to_string(), 1000);

        let expected =
            BankAccountEvent::deposited(ByteArray32([1; 32]), 1, "Satoshi".to_string(), 1000);
        let command =
            BankAccountCommand::deposited(ByteArray32([1; 32]), 1, "Satoshi".to_string(), 200);
        let services = BankAccountServices::new(Box::new(MockBankAccountServices::default()));

        AccountTestFramework::with(services)
            // Given this previously applied event
            .given(vec![previous])
            // When we fire this command
            .when(command)
            // Then we expect this resultant event
            .then_expect_events(vec![expected]);
    }

    #[test]
    fn test_withdraw_money() {
        let previous =
            BankAccountEvent::deposited(ByteArray32([0; 32]), 0, "Satoshi".to_string(), 200);
        let expected =
            BankAccountEvent::withdrew(ByteArray32([1; 32]), 1, "Satoshi".to_string(), 100);
        let services = MockBankAccountServices::default();
        services.set_atm_withdrawal_response(Ok(()));
        let command =
            BankAccountCommand::withdrew(ByteArray32([1; 32]), 1, "Satoshi".to_string(), 100);

        AccountTestFramework::with(BankAccountServices::new(Box::new(services)))
            .given(vec![previous])
            .when(command)
            .then_expect_events(vec![expected]);
    }

    #[test]
    fn test_withdraw_money_client_error() {
        let previous =
            BankAccountEvent::deposited(ByteArray32([0; 32]), 0, "Satoshi".to_string(), 200);
        let services = MockBankAccountServices::default();
        services.set_atm_withdrawal_response(Err(AtmError));
        let command = BankAccountCommand::Transaction {
            txid: ByteArray32([1; 32]),
            timestamp: 1,
            command: TransactionCommand::Withdraw {
                asset: "Satoshi".to_string(),
                amount: 100,
            },
        };

        let services = BankAccountServices::new(Box::new(services));
        AccountTestFramework::with(services)
            .given(vec![previous])
            .when(command)
            .then_expect_error_message("atm rule violation");
    }

    #[test]
    fn test_withdraw_money_funds_not_available() {
        let command =
            BankAccountCommand::withdrew(ByteArray32([1; 32]), 0, "Satoshi".to_string(), 200);

        let services = BankAccountServices::new(Box::new(MockBankAccountServices::default()));
        AccountTestFramework::with(services)
            .given_no_previous_events()
            .when(command)
            // Here we expect an error rather than any events
            .then_expect_error_message("funds not available")
    }

    #[test]
    fn test_lock_funds() {
        let previous =
            BankAccountEvent::deposited(ByteArray32([0; 32]), 0, "Satoshi".to_string(), 200);
        let expected = BankAccountEvent::funds_locked(
            ByteArray32([1; 32]),
            1,
            ByteArray32([0; 32]),
            "Satoshi".to_string(),
            100,
            86400,
        );
        let services = MockBankAccountServices::default();
        services.set_validate_check_response(Ok(()));
        let services = BankAccountServices::new(Box::new(services));

        let command = BankAccountCommand::funds_locked(
            ByteArray32([1; 32]),
            1,
            ByteArray32([0; 32]),
            "Satoshi".to_string(),
            100,
            86400,
        );

        AccountTestFramework::with(services)
            .given(vec![previous])
            .when(command)
            .then_expect_events(vec![expected]);
    }

    #[test]
    fn test_lock_funds_insufficient_funds() {
        let previous =
            BankAccountEvent::deposited(ByteArray32([0; 32]), 0, "Satoshi".to_string(), 200);
        let services = MockBankAccountServices::default();
        services.set_validate_check_response(Err(CheckingError));
        let services = BankAccountServices::new(Box::new(services));
        let command = BankAccountCommand::funds_locked(
            ByteArray32([1; 32]),
            1,
            ByteArray32([0; 32]),
            "Satoshi".to_string(),
            100,
            86400,
        );

        AccountTestFramework::with(services)
            .given(vec![previous])
            .when(command)
            .then_expect_error_message("check invalid");
    }

    #[test]
    fn test_unlock_funds_not_found() {
        let command =
            BankAccountCommand::funds_unlocked(ByteArray32([0; 32]), ByteArray32([0; 32]));

        let services = BankAccountServices::new(Box::new(MockBankAccountServices::default()));
        AccountTestFramework::with(services)
            .given_no_previous_events()
            .when(command)
            .then_expect_error_message("funds not available")
    }

    pub struct MockBankAccountServices {
        atm_withdrawal_response: Mutex<Option<Result<(), AtmError>>>,
        validate_check_response: Mutex<Option<Result<(), CheckingError>>>,
    }

    impl Default for MockBankAccountServices {
        fn default() -> Self {
            Self {
                atm_withdrawal_response: Mutex::new(None),
                validate_check_response: Mutex::new(None),
            }
        }
    }

    impl MockBankAccountServices {
        fn set_atm_withdrawal_response(&self, response: Result<(), AtmError>) {
            *self.atm_withdrawal_response.lock().unwrap() = Some(response);
        }
        fn set_validate_check_response(&self, response: Result<(), CheckingError>) {
            *self.validate_check_response.lock().unwrap() = Some(response);
        }
    }

    #[async_trait]
    impl BankAccountApi for MockBankAccountServices {
        async fn atm_withdrawal(&self, _atm_id: &str, _amount: f64) -> Result<(), AtmError> {
            self.atm_withdrawal_response.lock().unwrap().take().unwrap()
        }

        async fn validate_check(
            &self,
            _account_id: &str,
            _check_number: &str,
        ) -> Result<(), CheckingError> {
            self.validate_check_response.lock().unwrap().take().unwrap()
        }
    }
}

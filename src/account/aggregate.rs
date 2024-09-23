#![deny(arithmetic_overflow)]
use std::collections::{BTreeMap, VecDeque};
use std::mem;

use async_trait::async_trait;
use cqrs_es::Aggregate;
use serde::{Deserialize, Serialize};

use super::events::{AccountError, AccountEvent};
use crate::services::BankAccountServices;
use crate::util::types::ByteArray32;
use super::commands::{TransactionCommand, LifecycleCommand, AccountCommand};
use super::events::{LifecycleEvent, TransactionEvent};

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

    fn remove(&mut self, txid: &ByteArray32) -> Option<u64> {
        if let Some(timestamp) = self.txids.remove(txid) {
            self.timeseries.retain(|(_, t)| t != txid);
            Some(timestamp)
        } else {
            None
        }
    }
}

#[derive(Serialize, Deserialize)]
struct ReservedFunds {
    asset: String,
    amount: u64,
}

#[derive(Serialize, Deserialize, Default)]
pub enum Account {
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

    fn save_txid(&mut self, txid: ByteArray32, timestamp: u64) {
        self.processed_transactions
            .insert(txid, timestamp)
            .expect("txid already exists");
    }

    fn remove_txid(&mut self, txid: &ByteArray32) {
        self.processed_transactions
            .remove(txid)
            .expect("txid does not exist");
    }
}

#[async_trait]
impl Aggregate for Account {
    type Command = AccountCommand;
    type Event = AccountEvent;
    type Error = AccountError;
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
            AccountCommand::Lifecycle(command) => match command {
                LifecycleCommand::Open { account_id } => match self {
                    Account::Uninitialized | Account::Closed => {
                        Ok(vec![AccountEvent::account_opened(account_id)])
                    }
                    _ => Err(AccountError::AccountAlreadyExists),
                },
                LifecycleCommand::Disable => {
                    if let Account::InService { .. } = self {
                        Ok(vec![AccountEvent::account_disabled()])
                    } else {
                        Err(AccountError::AccountNotInService)
                    }
                }
                LifecycleCommand::Enable => {
                    if let Account::Disabled { .. } = self {
                        Ok(vec![AccountEvent::account_enabled()])
                    } else {
                        Err(AccountError::AccountNotDisabled)
                    }
                }
                LifecycleCommand::Close => match self {
                    Account::Uninitialized | Account::Closed => {
                        Err(AccountError::AccountNotFound)
                    }
                    Account::InService { state } => {
                        if state.is_empty() {
                            Ok(vec![AccountEvent::account_closed()])
                        } else {
                            Err(AccountError::AccountNotEmpty)
                        }
                    }
                    Account::Disabled { state } => {
                        if state.is_empty() {
                            Ok(vec![AccountEvent::account_closed()])
                        } else {
                            Err(AccountError::AccountNotEmpty)
                        }
                    }
                },
            },
            AccountCommand::Transaction {
                txid,
                timestamp,
                command,
            } => match self {
                Account::Uninitialized | Account::Closed => {
                    Err(AccountError::AccountNotFound)
                }
                Account::Disabled { .. } => Err(AccountError::AccountNotInService),
                Account::InService { state } => {
                    match command {
                        TransactionCommand::Deposit { asset, amount } => {
                            if let Some(timestamp) =
                                state.processed_transactions.get_timestamp(&txid)
                            {
                                return Err(AccountError::DuplicateTransaction(timestamp));
                            }
                            Ok(vec![AccountEvent::deposited(
                                txid, timestamp, asset, amount,
                            )])
                        }
                        TransactionCommand::Withdraw { asset, amount } => {
                            if let Some(timestamp) =
                                state.processed_transactions.get_timestamp(&txid)
                            {
                                return Err(AccountError::DuplicateTransaction(timestamp));
                            }
                            if state.assets.get(&asset).unwrap_or(&0) < &amount {
                                return Err(AccountError::InsufficientFunds);
                            }

                            Ok(vec![AccountEvent::withdrew(
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
                                return Err(AccountError::DuplicateTransaction(timestamp));
                            }
                            Ok(vec![AccountEvent::credited(
                                txid,
                                timestamp,
                                from_account,
                                asset,
                                amount,
                            )])
                        }
                        TransactionCommand::ReverseCredit {
                            from_account,
                            asset,
                            amount,
                        } => {
                            if let Some(timestamp) =
                                state.processed_transactions.get_timestamp(&txid)
                            {
                                return Ok(vec![AccountEvent::credit_reversed(
                                    txid,
                                    timestamp,
                                    from_account,
                                    asset,
                                    amount,
                                )]);
                            }
                            Err(AccountError::TransactionNotFound)
                        }
                        TransactionCommand::ReverseDebit {
                            to_account,
                            asset,
                            amount,
                        } => {
                            if let Some(timestamp) =
                                state.processed_transactions.get_timestamp(&txid)
                            {
                                return Ok(vec![AccountEvent::debit_reversed(
                                    txid, timestamp, to_account, asset, amount,
                                )]);
                            }
                            Err(AccountError::TransactionNotFound)
                        }
                        TransactionCommand::Debit {
                            to_account,
                            asset,
                            amount,
                        } => {
                            if let Some(timestamp) =
                                state.processed_transactions.get_timestamp(&txid)
                            {
                                return Err(AccountError::DuplicateTransaction(timestamp));
                            }
                            if state.assets.get(&asset).unwrap_or(&0) < &amount {
                                return Err(AccountError::InsufficientFunds);
                            }

                            Ok(vec![AccountEvent::debited(
                                txid, timestamp, to_account, asset, amount,
                            )])
                        }
                        TransactionCommand::LockFunds {
                            asset,
                            amount,
                        } => {
                            if state.reserving.contains_key(&txid) {
                                return Err(AccountError::DuplicateLock);
                            }
                            if state.assets.get(&asset).unwrap_or(&0) < &amount {
                                return Err(AccountError::InsufficientFunds);
                            }

                            Ok(vec![AccountEvent::funds_locked(
                                txid, timestamp, asset, amount,
                            )])
                        }
                        TransactionCommand::UnlockFunds => {
                            if let Some(locked) = state.reserving.get(&txid) {
                                Ok(vec![AccountEvent::funds_unlocked(
                                    txid, timestamp, locked.asset.clone(), locked.amount,
                                )])
                            } else {
                                Err(AccountError::LockNotFound)
                            }
                        }
                        TransactionCommand::Settle {
                            to_account, receive_asset, receive_amount,
                        } => {
                            if let Some(timestamp) =
                                state.processed_transactions.get_timestamp(&txid)
                            {
                                return Err(AccountError::DuplicateTransaction(timestamp));
                            }

                            let Some(locked) = state.reserving.get(&txid) else {
                                return Err(AccountError::LockNotFound)
                            };
                            Ok(vec![AccountEvent::settlement(
                                txid,
                                timestamp,
                                to_account,
                                locked.asset.clone(),
                                locked.amount,
                                receive_asset,
                                receive_amount
                            )])
                        }
                    }
                }
            },
        }
    }

    fn apply(&mut self, event: Self::Event) {
        match event {
            AccountEvent::Lifecycle(account_event) => match account_event {
                LifecycleEvent::AccountOpened { account_id } => {
                    *self = Account::InService {
                        state: BankAccountState {
                            account_id,
                            assets: BTreeMap::new(),
                            reserving: BTreeMap::new(),
                            processed_transactions: ProcessedTransactions::new(DEFAULT_TTL),
                        },
                    };
                }
                LifecycleEvent::AccountDisabled => {
                    let Account::InService { state } = self else {
                        unreachable!("account should be in service");
                    };
                    let mut temp = BankAccountState::default();
                    mem::swap(state, &mut temp);
                    *self = Account::Disabled { state: temp };
                }
                LifecycleEvent::AccountEnabled => {
                    let Account::Disabled { state } = self else {
                        unreachable!("account should be disabled");
                    };
                    let mut temp = BankAccountState::default();
                    mem::swap(state, &mut temp);
                    *self = Account::InService { state: temp };
                }
                LifecycleEvent::AccountClosed => {
                    *self = Account::Closed;
                }
            },
            AccountEvent::Transaction {
                timestamp,
                txid,
                event,
            } => {
                let Account::InService { ref mut state } = self else {
                    unreachable!("account should be in service");
                };

                match event {
                    TransactionEvent::Deposited { asset, amount } => {
                        state.save_txid(txid, timestamp);
                        let balance = state.assets.entry(asset.to_owned()).or_insert(0);
                        *balance = balance
                            .checked_add(amount)
                            .expect("balance should not overflow");
                    }
                    TransactionEvent::Withdrew { asset, amount } => {
                        state.save_txid(txid, timestamp);
                        let balance = state.assets.entry(asset.to_owned()).or_insert(0);
                        *balance = balance
                            .checked_sub(amount)
                            .expect("balance should not be negative");
                    }
                    TransactionEvent::Debited { asset, amount, .. } => {
                        state.save_txid(txid, timestamp);
                        let balance = state.assets.entry(asset.to_owned()).or_insert(0);
                        *balance = balance
                            .checked_sub(amount)
                            .expect("balance should not be negative");
                    }
                    TransactionEvent::DebitReversed { asset, amount, .. } => {
                        state.remove_txid(&txid);
                        let balance = state.assets.entry(asset.to_owned()).or_insert(0);
                        *balance = balance
                            .checked_add(amount)
                            .expect("balance should not overflow");
                    }
                    TransactionEvent::Credited { asset, amount, .. } => {
                        state.save_txid(txid, timestamp);
                        let balance = state.assets.entry(asset.to_owned()).or_insert(0);
                        *balance = balance
                            .checked_add(amount)
                            .expect("balance should not overflow");
                    }
                    TransactionEvent::CreditReversed { asset, amount, .. } => {
                        state.remove_txid(&txid);
                        let balance = state.assets.entry(asset.to_owned()).or_insert(0);
                        *balance = balance
                            .checked_sub(amount)
                            .expect("balance should not be negative");
                    }
                    TransactionEvent::FundsLocked {
                        asset,
                        amount,
                    } => {
                        let balance = state.assets.entry(asset.to_owned()).or_insert(0);
                        *balance = balance
                            .checked_sub(amount)
                            .expect("balance should not be negative");

                        state.reserving.insert(
                            txid,
                            ReservedFunds {
                                asset,
                                amount,
                            },
                        );
                    }
                    TransactionEvent::FundsUnlocked { .. } => {
                        let reserved = state
                            .reserving
                            .remove(&txid)
                            .expect("txid not found in reserving");
                        let balance = state.assets.entry(reserved.asset).or_insert(0);
                        *balance = balance
                            .checked_add(reserved.amount)
                            .expect("balance should not overflow");
                    }
                    TransactionEvent::Settled { .. } => {
                        state.save_txid(txid, timestamp);
                        state
                            .reserving
                            .remove(&txid)
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

    use crate::account::aggregate::Account;
    use crate::account::commands::{AccountCommand, TransactionCommand};
    use crate::account::events::AccountEvent;
    use crate::services::{AtmError, BankAccountApi, BankAccountServices, CheckingError};
    use crate::util::types::ByteArray32;

    // A test framework that will apply our events and command
    // and verify that the logic works as expected.
    type AccountTestFramework = TestFramework<Account>;

    #[test]
    fn test_deposit_money() {
        let expected =
            AccountEvent::deposited(ByteArray32([0; 32]), 0, "Satoshi".to_string(), 1000);
        let command =
            AccountCommand::deposited(ByteArray32([0; 32]), 0, "Satoshi".to_string(), 1000);

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
            AccountEvent::deposited(ByteArray32([0; 32]), 0, "Satoshi".to_string(), 1000);

        let expected =
            AccountEvent::deposited(ByteArray32([1; 32]), 1, "Satoshi".to_string(), 1000);
        let command =
            AccountCommand::deposited(ByteArray32([1; 32]), 1, "Satoshi".to_string(), 200);
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
            AccountEvent::deposited(ByteArray32([0; 32]), 0, "Satoshi".to_string(), 200);
        let expected =
            AccountEvent::withdrew(ByteArray32([1; 32]), 1, "Satoshi".to_string(), 100);
        let services = MockBankAccountServices::default();
        services.set_atm_withdrawal_response(Ok(()));
        let command =
            AccountCommand::withdrew(ByteArray32([1; 32]), 1, "Satoshi".to_string(), 100);

        AccountTestFramework::with(BankAccountServices::new(Box::new(services)))
            .given(vec![previous])
            .when(command)
            .then_expect_events(vec![expected]);
    }

    #[test]
    fn test_withdraw_money_client_error() {
        let previous =
            AccountEvent::deposited(ByteArray32([0; 32]), 0, "Satoshi".to_string(), 200);
        let services = MockBankAccountServices::default();
        services.set_atm_withdrawal_response(Err(AtmError));
        let command = AccountCommand::Transaction {
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
            AccountCommand::withdrew(ByteArray32([1; 32]), 0, "Satoshi".to_string(), 200);

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
            AccountEvent::deposited(ByteArray32([0; 32]), 0, "Satoshi".to_string(), 200);
        let expected = AccountEvent::funds_locked(
            ByteArray32([1; 32]),
            1,
            "Satoshi".to_string(),
            100,
        );
        let services = MockBankAccountServices::default();
        services.set_validate_check_response(Ok(()));
        let services = BankAccountServices::new(Box::new(services));

        let command = AccountCommand::lock_funds(
            ByteArray32([1; 32]),
            1,
            "Satoshi".to_string(),
            100,
        );

        AccountTestFramework::with(services)
            .given(vec![previous])
            .when(command)
            .then_expect_events(vec![expected]);
    }

    #[test]
    fn test_lock_funds_insufficient_funds() {
        let previous =
            AccountEvent::deposited(ByteArray32([0; 32]), 0, "Satoshi".to_string(), 200);
        let services = MockBankAccountServices::default();
        services.set_validate_check_response(Err(CheckingError));
        let services = BankAccountServices::new(Box::new(services));
        let command = AccountCommand::lock_funds(
            ByteArray32([1; 32]),
            1,
            "Satoshi".to_string(),
            100,
        );

        AccountTestFramework::with(services)
            .given(vec![previous])
            .when(command)
            .then_expect_error_message("check invalid");
    }

    #[test]
    fn test_unlock_funds_not_found() {
        let command =
            AccountCommand::unlock_funds(ByteArray32([0; 32]));

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

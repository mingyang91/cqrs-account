use std::sync::Mutex as StdMutex;
use std::time::Duration;
use std::{collections::BTreeMap, sync::Arc};
use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;
use futures::{Stream, StreamExt, TryFutureExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use sqlx::{query, Pool, Postgres};
use stm::TVar;
use tokio::sync::oneshot;
use tokio::time::sleep;
use tokio_stream::wrappers::ReceiverStream;
use crate::util::types::ByteArray32;

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct AssetID(u32);

#[derive(thiserror::Error, Debug)]
pub enum AssetError {
    #[error("Asset not registered")]
    NotRegistered
}

impl FromStr for AssetID {
    type Err = AssetError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "BTC" => Ok(AssetID(0)),
            "ETH" => Ok(AssetID(1)),
            _ => Err(AssetError::NotRegistered)
        }
    }
}

pub struct Balance(TVar<u64>);

impl Default for Balance {
    fn default() -> Self {
        Balance(TVar::new(0))
    }
}

impl Balance {
    fn new(value: u64) -> Self {
        Balance(TVar::new(value))
    }
}


#[derive(Default)]
pub struct Account {
    pub assets: StdMutex<BTreeMap<AssetID, Balance>>,
    pub locked_assets: StdMutex<BTreeMap<ByteArray32, (AssetID, u64)>>,
    pub unspendable_assets: StdMutex<BTreeMap<AssetID, Balance>>,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Insufficient funds")]
    InsufficientFunds,
    #[error("Lock not found")]
    LockNotFound,
    #[error("Failed to persist transaction: {0}")]
    Persist(#[from] sqlx::Error)
}

impl Account {
    pub fn credit(&self, asset: AssetID, amount: u64) {
        let mut assets = self.assets.lock().expect("Failed to lock assets");
        let entry = assets.entry(asset).or_default();
        stm::atomically(|t| {
            entry.0.modify(t, |v| v + amount)
        });
    }

    pub fn debit(&self, asset: AssetID, amount: u64) -> Result<(), Error> {
        let mut assets = self.assets.lock().expect("Failed to lock assets");
        let entry = assets.entry(asset).or_default();
        stm::atomically(|t| {
            if entry.0.read(t)? < amount {
                return Ok(Err(Error::InsufficientFunds))
            }
            entry.0.modify(t, |v| v - amount)?;
            Ok(Ok(()))
        })
    }

    pub fn lock(&self, id: ByteArray32, asset: AssetID, amount: u64) -> Result<(), Error> {
        let mut locked_assets = self.locked_assets.lock().expect("Failed to lock locked assets");
        if locked_assets.contains_key(&id) {
            return Ok(())
        }
        let mut assets = self.assets.lock().expect("Failed to lock assets");
        let entry = assets.entry(asset).or_default();
        stm::atomically(|t| {
            if entry.0.read(t)? < amount {
                return Ok(Err(Error::InsufficientFunds))
            }
            entry.0.modify(t, |v| v - amount)?;
            Ok(Ok(()))
        })?;

        locked_assets.insert(id, (asset, amount));
        Ok(())
    }

    pub fn unlock(&self, id: ByteArray32) -> Result<(), Error> {
        let mut locked_assets = self.locked_assets.lock().expect("Failed to lock locked assets");
        let Some((asset, amount)) = locked_assets.remove(&id) else {
            return Ok(());
        };

        let mut assets = self.assets.lock().expect("Failed to lock assets");
        let entry = assets.entry(asset).or_default();
        stm::atomically(|t| {
            entry.0.modify(t, |v| v + amount)?;
            Ok(Ok(()))
        })
    }
}

#[derive(Serialize, Deserialize, PartialEq, PartialOrd, Eq, Ord, Clone)]
pub struct AccountID(String);

pub struct AccountBook {
    pub accounts: StdMutex<BTreeMap<AccountID, Arc<Account>>>,
    pub store: PostgresStore,
}

impl AccountBook {
    pub async fn new() -> Self {
        let pool = Pool::connect("postgres://postgres:postgres@localhost:5432/postgres")
            .await
            .expect("Failed to connect to database");
        AccountBook {
            accounts: Default::default(),
            store: PostgresStore::new(pool)
        }
    }

    fn get(&self, id: &AccountID) -> Arc<Account> {
        let mut lock = self.accounts.lock().expect("Failed to lock account book");
        lock.entry(id.clone())
            .or_default()
            .clone()
    }

    pub async fn deposit(&self,
                         txid: ByteArray32,
                         account_id: &AccountID,
                         asset: AssetID,
                         amount: u64) {
        let account = self.get(&account_id);
        let tx = Transaction {
            id: txid,
            data: TransactionData::Deposit {
                account: account_id.clone(),
                amount,
                asset,
            }
        };

        while let Err(e) = self.store.persist(tx.clone()).await {
            tracing::warn!("Failed to persist transaction: {:?}, retrying", e);
            sleep(Duration::from_secs(1)).await;
        }

        account.credit(asset, amount);
    }

    pub async fn transfer(&self, 
                          txid: ByteArray32,
                          from: &AccountID,
                          to: &AccountID, 
                          asset: AssetID, 
                          amount: u64) -> Result<(), Error> {
        let from_account = self.get(from);
        let to_account = self.get(to);
        let tx = Transaction {
            id: txid,
            data: TransactionData::Transfer {
                from_account: from.clone(),
                to_account: to.clone(),
                asset,
                amount,
            }
        };

        while let Err(e) = self.store.persist(tx.clone()).await {
            tracing::warn!("Failed to persist transaction: {:?}, retrying", e);
            sleep(Duration::from_secs(1)).await;
        }

        from_account.debit(asset, amount)?;
        to_account.credit(asset, amount);
        Ok(())
    }

    pub async fn lock(&self, 
                      txid: ByteArray32,
                      account_id: &AccountID,
                      asset: AssetID,
                      amount: u64) -> Result<(), Error> {
        let account = self.get(account_id);

        let tx = Transaction {
            id: txid,
            data: TransactionData::Lock {
                id: txid,
                account: account_id.clone(),
                asset,
                amount,
            }
        };

        while let Err(e) = self.store.persist(tx.clone()).await {
            tracing::warn!("Failed to persist transaction: {:?}, retrying", e);
            sleep(Duration::from_secs(1)).await;
        }

        account.lock(txid, asset, amount)?;
        Ok(())
    }

    pub async fn unlock(&self, 
                        txid: ByteArray32,
                        account_id: &AccountID) -> Result<(), Error> {
        let account = self.get(account_id);

        let tx = Transaction {
            id: txid,
            data: TransactionData::Unlock {
                id: txid,
            }
        };

        while let Err(e) = self.store.persist(tx.clone()).await {
            tracing::warn!("Failed to persist transaction: {:?}, retrying", e);
            sleep(Duration::from_secs(1)).await;
        }

        account.unlock(txid)?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Transaction {
    pub id: ByteArray32,
    pub data: TransactionData
}

#[derive(Serialize, Deserialize, Clone)]
pub enum TransactionData {
    Deposit {
        account: AccountID,
        asset: AssetID,
        amount: u64
    },
    Transfer {
        from_account: AccountID,
        to_account: AccountID,
        asset: AssetID,
        amount: u64,
    },
    Lock {
        id: ByteArray32,
        account: AccountID,
        asset: AssetID,
        amount: u64,
    },
    Unlock {
        id: ByteArray32,
    },
}

pub trait Store {
    type Item: Send;
    type Error;

    fn persist(&self, item: Self::Item) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.persist_all(vec![item])
            .and_then(|_| async { Ok(()) })
    }

    fn persist_all<I: IntoIterator<Item = Self::Item> + Send>(&self, items: I) -> impl Future<Output = Result<u64, Self::Error>> + Send;
    fn load_all(&self) -> Pin<Box<dyn Stream<Item = Result<Self::Item, Self::Error>> + Send + '_>>;
}

#[derive(Clone)]
pub struct PostgresStore {
    pool: Pool<Postgres>,
    tx: tokio::sync::mpsc::Sender<(Transaction, tokio::sync::oneshot::Sender<Result<(), Arc<sqlx::Error>>>)>,
}

impl PostgresStore {
    pub fn new(pool: Pool<Postgres>) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(1024);
        let this = Self {
            pool,
            tx
        };

        let bind = this.clone();
        tokio::spawn(async move {
            bind.background(rx).await;
        });
        this
    }

    async fn flush<I: IntoIterator<Item=Transaction>>(&self, items: I) -> Result<u64, sqlx::Error> {
        let (ids, data): (Vec<String>, Vec<Vec<u8>>) = items
            .into_iter()
            .map(|item| {
                let id = hex::encode(item.id.0);
                let data = bincode::serialize(&item.data).expect("Failed to serialize transaction data");
                (id, data)
            })
            .unzip();
        let res = query!(
            "
            INSERT INTO transactions (id, data)
            SELECT * FROM UNNEST($1::TEXT[], $2::BYTEA[])
            ON CONFLICT DO NOTHING
            ",
            &ids,
            &data
        )
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected())
    }

    async fn enqueue(&self, item: Transaction) -> Result<(), Arc<sqlx::Error>> {
        let (tx, rx) = oneshot::channel();
        let _ = self.tx.send((item, tx)).await.expect("Failed to send transaction to queue");
        rx.await.expect("Failed to receive transaction response")
    }

    async fn background(&self, rx: tokio::sync::mpsc::Receiver<(Transaction, tokio::sync::oneshot::Sender<Result<(), Arc<sqlx::Error>>>)>) {
        let stream: ReceiverStream<_> = rx.into();
        let mut chunked = stream.ready_chunks(1024);

        while let Some(chunks) = chunked.next().await {
            let (items, promises): (Vec<Transaction>, Vec<oneshot::Sender<Result<(), Arc<sqlx::Error>>>>) = chunks.into_iter().unzip();
            let res = self.flush(items).await.map(|_| ()).map_err(Arc::new);
            for p in promises {
                let _ = p.send(res.clone());
            }
        }
    }
}

impl Store for PostgresStore {
    type Item = Transaction;
    type Error = Arc<sqlx::Error>;

    async fn persist(&self, item: Self::Item) -> Result<(), Self::Error> {
        self.enqueue(item).await
    }

    async fn persist_all<I: IntoIterator<Item=Self::Item>>(&self, items: I) -> Result<u64, Self::Error> {
        todo!()
    }

    fn load_all(&self) -> Pin<Box<dyn Stream<Item = Result<Self::Item, Self::Error>> + Send + '_>> {
        let stream = query!("SELECT id, data FROM transactions")
            .fetch(&self.pool)
            .map_ok(|row| {
                let id: [u8; 32] = hex::decode(row.id).expect("Invalid transaction ID")[..32].try_into().expect("Invalid transaction ID");
                let data = bincode::deserialize(&row.data).expect("Failed to deserialize transaction data");
                Transaction {
                    id: ByteArray32(id),
                    data,
                }
            })
            .map_err(Arc::new);

        Box::pin(stream)
    }
}

#[cfg(test)]
mod test {
    use std::{sync::{atomic::{AtomicUsize, Ordering}, Arc}, time::Instant};

    use futures::future::join_all;
    use rand::{random, Rng};
    use sqlx::postgres::PgPoolOptions;

    use crate::{simple::{AccountBook, AccountID, PostgresStore}, util::types::ByteArray32};

    use super::Error;


    #[tokio::test]
    async fn test() {
        tracing_subscriber::fmt::init();
        let pool = PgPoolOptions::new()
            .max_connections(128)
            .connect("postgres://postgres:postgres@127.0.0.1:5432/postgres")
            .await
            .expect("Failed to connect to database");

        let book = Arc::new(AccountBook {
            accounts: Default::default(),
            store: PostgresStore::new(pool)
        });

        let BTC = "BTC".parse().expect("Failed to parse asset");
        let ETH = "ETH".parse().expect("Failed to parse asset");

        for i in 0..1000 {
            let account_id = AccountID(format!("ACCT-{:04}", i));
            let txid = ByteArray32(random());
            let amount = rand::thread_rng().gen_range(10_000u64..1_000_000u64);
            book.deposit(txid, &account_id, BTC, amount).await;
            let amount = rand::thread_rng().gen_range(10_000u64..1_000_000u64);
            book.deposit(txid, &account_id, ETH, amount).await;
        }
    
        let start = Instant::now();
    
        let success = Arc::new(AtomicUsize::new(0));
        let mut tasks = vec![];
        for _ in 0..128 {
            let success = success.clone();
            let book = book.clone();
            tasks.push(tokio::spawn(async move {
                for i in 0u32..1000u32 {
                    let offset: u32 = rand::thread_rng().gen_range(0u32..999u32);
                    let bid = (i + offset) % 1000;
                    let seller = AccountID(format!("ACCT-{:04}", i));
                    let buyer = AccountID(format!("ACCT-{:04}", bid));
                    if let Err(e) = order(&book, &seller, &buyer).await {
                        eprintln!("Error: {:?}", e);
                        continue
                    };
                    success.fetch_add(1, Ordering::Relaxed);
                }
            }));
        }
    
        join_all(tasks).await;
    
        println!("Elapsed time: {:?}, success: {}", start.elapsed(), success.fetch_add(0, Ordering::Relaxed));
    }

    async fn order(book: &AccountBook, 
                   seller: &AccountID, 
                   buyer: &AccountID) -> Result<(), Error> {
        let BTC = "BTC".parse().expect("Failed to parse asset");
        let ETH = "ETH".parse().expect("Failed to parse asset");
        let txid = ByteArray32(random());
        let sell_amount = rand::thread_rng().gen_range(1u64..100u64);
        let buy_amount = rand::thread_rng().gen_range(1u64..100u64);
        book.transfer(txid, seller, buyer, BTC, sell_amount).await?;
        let txid = ByteArray32(random());
        book.transfer(txid, buyer, seller, ETH, buy_amount).await
    }
}
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use futures::future::join_all;
use reqwest::Client;
use serde_json::json;
use rand::{random, Rng};
use tokio::time::Instant;
use cqrs_account::util::types::ByteArray32;

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error("Reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("Domain error: {0}")]
    Domain(String)
}

#[tokio::main]
async fn main() {
    let client = Client::new();

    for i in 0..1000 {
        let account_id = format!("ACCT-{:04}", i);
        create_account(&client, &account_id).await.unwrap();
        deposit_init_money(&client, &account_id).await.unwrap();
    }

    let start = Instant::now();

    let success = Arc::new(AtomicUsize::new(0));
    let mut tasks = vec![];
    for _ in 0..32 {
        let client = client.clone();
        let success = success.clone();
        tasks.push(tokio::spawn(async move {
            for i in 0u32..1000u32 {
                let offset: u32 = random();
                let bid = (i + offset) % 999;
                let seller = format!("ACCT-{:04}", i);
                let buyer = format!("ACCT-{:04}", bid);
                if let Err(e) = order(&client, &seller, &buyer).await {
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


async fn create_account(client: &Client, account_id: &str) -> Result<(), reqwest::Error> {
    let url = format!("http://localhost:3030/account/{}", account_id);
    let body = json!({
        "Lifecycle": {
            "Open": {
                "account_id": account_id
            }
        }
    });
    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await?;
    response.error_for_status()
        .map(|_| ())
}

async fn deposit_money(client: &Client, account_id: &str, asset: &str, amount: u64) -> Result<(), Error> {
    let url = format!("http://localhost:3030/account/{}", account_id);
    let txid = ByteArray32(random());
    let now = chrono::Utc::now().timestamp() as u64;
    let body = json!({
            "Transaction": {
                "command": {
                    "Deposit": {
                        "asset": asset,
                        "amount": amount
                    }
                },
                "timestamp": now,
                "txid": txid
            }
    });

    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await?;
    if response.status() == 204 {
        Ok(())
    } else {
        let error_message = response.text().await?;
        Err(Error::Domain(error_message))
    }
}

async fn deposit_init_money(client: &Client, account_id: &str) -> Result<(), Error> {
    let amount = rand::thread_rng().gen_range(10_000u64..1_000_000u64);
    deposit_money(client, account_id, "BTC", amount).await?;
    let amount = rand::thread_rng().gen_range(10_000u64..1_000_000u64);
    deposit_money(client, account_id, "ETH", amount).await
}


async fn order(client: &Client, seller: &str, buyer: &str) -> Result<(), Error> {
    let txid = ByteArray32(random());
    let sell_asset = "BTC";
    let sell_amount = rand::thread_rng().gen_range(1u64..100u64);
    let buy_asset = "ETH";
    let buy_amount = rand::thread_rng().gen_range(1u64..100u64);

    place_order(client, seller, txid, sell_asset, sell_amount, buy_asset, buy_amount).await?;

    let mut retry = 0;
    loop {
        let res = continue_order(client, txid).await;
        retry += 1;
        if retry % 10 == 0 {
            eprintln!("Retry place {} times, {:?}", retry, res);
        }
        if let Err(Error::Domain(msg)) = res {
            if msg.contains("Placed") {
                break
            }
        }
    }

    buy_order(client, txid, buyer).await?;
    let mut retry = 0;
    loop {
        let res = continue_order(client, txid).await;
        retry += 1;
        if retry % 10 == 0 {
            eprintln!("Retry buy {} times, {:?}", retry, res);
        }
        if let Err(Error::Domain(msg)) = res {
            if msg.contains("Settled") {
                break
            }
        }
    }
    Ok(())
}

async fn place_order(client: &Client,
                     seller: &str,
                     txid: ByteArray32,
                     sell_asset: &str,
                     sell_amount: u64,
                     buy_asset: &str,
                     buy_amount: u64) -> Result<(), Error> {
    let url = format!("http://localhost:3030/order/{}", txid.hex());
    let now = chrono::Utc::now().timestamp() as u64;
    let body = json!({
        "Open": {
            "config": {
                "order_id": txid,
                "seller": seller,
                "sell_asset": sell_asset,
                "sell_amount": sell_amount,
                "buy_asset": buy_asset,
                "buy_amount": buy_amount,
                "timestamp": now
            }
        }
    });

    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await?;
    if response.status() == 204 {
        Ok(())
    } else {
        let error_message = response.text().await?;
        Err(Error::Domain(error_message))
    }
}

async fn continue_order(client: &Client,
                        txid: ByteArray32) -> Result<(), Error> {
    let url = format!("http://localhost:3030/order/{}", txid.hex());
    let body = json!({
        "Continue": null
    });

    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await?;
    if response.status() == 204 {
        Ok(())
    } else {
        let error_message = response.text().await?;
        Err(Error::Domain(error_message))
    }
}

async fn buy_order(client: &Client,
                   txid: ByteArray32,
                   buyer: &str) -> Result<(), Error> {
    let url = format!("http://localhost:3030/order/{}", txid.hex());
    let now = chrono::Utc::now().timestamp() as u64;
    let body = json!({
        "Buy": {
            "buyer": buyer,
            "timestamp": now
        }
    });

    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await?;
    if response.status() == 204 {
        Ok(())
    } else {
        let error_message = response.text().await?;
        Err(Error::Domain(error_message))
    }
}
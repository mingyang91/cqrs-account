use std::future::Future;

pub struct TransactionGuard<Fut>
where
    Fut: core::future::Future<Output = ()> + Send + 'static,
{
    redo: Option<Fut>,
}

impl<Fut> Drop for TransactionGuard<Fut>
where
    Fut: Future<Output = ()> + Send + 'static,
{
    fn drop(&mut self) {
        if let Some(redo) = self.redo.take() {
            tokio::spawn(redo);
        }
    }
}

impl<Fut> TransactionGuard<Fut>
where
    Fut: Future<Output = ()> + Send + 'static,
{
    pub fn new(redo: Fut) -> Self {
        Self { redo: Some(redo) }
    }

    pub fn commit(mut self) {
        self.redo = None;
    }
}

#[cfg(test)]
mod test {
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    use tokio::time::sleep;

    #[tokio::test]
    async fn test_transaction_guard() {
        let redo = Arc::new(Mutex::new(false));
        {
            let redo = redo.clone();
            let _guard = super::TransactionGuard::new(async move {
                *redo.clone().lock().unwrap() = true;
            });
        }
        sleep(Duration::from_secs(1)).await;
        assert!(*redo.lock().unwrap());
    }

    #[tokio::test]
    async fn test_transaction_guard_commit() {
        let redo = Arc::new(Mutex::new(false));
        {
            let redo = redo.clone();
            let guard = super::TransactionGuard::new(async move {
                *redo.clone().lock().unwrap() = true;
            });
            guard.commit();
        }
        sleep(Duration::from_secs(1)).await;
        assert!(!*redo.lock().unwrap());
    }
}

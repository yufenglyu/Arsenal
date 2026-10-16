use futures_util::future::{select, Either};
use std::{
    collections::HashMap,
    future::Future,
    sync::{Mutex, OnceLock},
};
use tokio::sync::watch;

fn operations() -> &'static Mutex<HashMap<String, watch::Sender<bool>>> {
    static OPERATIONS: OnceLock<Mutex<HashMap<String, watch::Sender<bool>>>> = OnceLock::new();
    OPERATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub struct Operation {
    id: String,
    signal: watch::Sender<bool>,
}
impl Operation {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn start(id: String) -> Self {
        let signal = operations()
            .lock()
            .unwrap()
            .entry(id.clone())
            .or_insert_with(|| watch::channel(false).0)
            .clone();
        Self { id, signal }
    }
    pub fn check(&self) -> Result<(), String> {
        if *self.signal.borrow() {
            Err("Operation cancelled".into())
        } else {
            Ok(())
        }
    }
    pub async fn during<T>(
        &self,
        future: impl Future<Output = Result<T, String>>,
    ) -> Result<T, String> {
        self.check()?;
        let mut receiver = self.signal.subscribe();
        let cancelled = async move {
            while !*receiver.borrow_and_update() {
                if receiver.changed().await.is_err() {
                    break;
                }
            }
        };
        match select(Box::pin(future), Box::pin(cancelled)).await {
            Either::Left((result, _)) => {
                self.check()?;
                result
            }
            Either::Right(_) => Err("Operation cancelled".into()),
        }
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        operations().lock().unwrap().remove(&self.id);
    }
}

#[tauri::command]
pub fn cancel_github_operation(operation_id: String) {
    // A stop request may arrive before the import command is first polled.
    let mut entries = operations().lock().unwrap();
    if entries.len() >= 256 {
        // Expire late/early cancellation tombstones; live operations own their signal.
        entries.retain(|_, signal| !*signal.borrow());
    }
    entries
        .entry(operation_id)
        .or_insert_with(|| watch::channel(false).0)
        .send_replace(true);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cancellation_interrupts_a_waiting_download_and_cleans_registry() {
        let operation = Operation::start("waiting-test".into());
        let result = operation
            .during(async {
                cancel_github_operation("waiting-test".into());
                std::future::pending::<Result<(), String>>().await
            })
            .await;
        assert_eq!(result.unwrap_err(), "Operation cancelled");
        drop(operation);
        assert!(!operations().lock().unwrap().contains_key("waiting-test"));
    }
    #[test]
    fn early_stop_is_preserved_until_the_operation_starts() {
        cancel_github_operation("early-test".into());
        let operation = Operation::start("early-test".into());
        assert!(operation.check().is_err());
    }
}

use notify::{event::ModifyKind, Config as NotifyConfig, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::{sync::mpsc, task::JoinHandle, time};

#[derive(Debug, thiserror::Error)]
pub enum WalletConfigsError {
    #[error("Failed to read wallet configs file: {0}")]
    ReadFile(#[from] std::io::Error),
    #[error("Failed to parse wallet configs JSON: {0}")]
    ParseJson(#[from] serde_json::Error),
    #[error("Failed to initialize file watcher: {0}")]
    Watcher(#[from] notify::Error),
    #[error("Failed to read wallet configs: {0}")]
    ReadLock(String),
    #[error("Failed to get parent directory")]
    ParentDirectory,
}

#[derive(Debug)]
pub struct WalletConfigService {
    wallet_configs: Arc<RwLock<Value>>,
    _watcher: RecommendedWatcher,
    _watch_task: JoinHandle<()>,
}

impl WalletConfigService {
    fn is_reload_event_kind(kind: &EventKind) -> bool {
        match kind {
            EventKind::Create(_) | EventKind::Modify(ModifyKind::Name(_)) => true,
            EventKind::Modify(modify_kind) => !matches!(modify_kind, ModifyKind::Metadata(_) | ModifyKind::Other),
            _ => false,
        }
    }

    pub fn new(file_path: impl Into<PathBuf>) -> Result<Self, WalletConfigsError> {
        let file_path = file_path.into();

        let configs = Self::read_flags_from_file_sync(&file_path)?;
        let wallet_configs = Arc::new(RwLock::new(configs));

        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut watcher = RecommendedWatcher::new(
            move |result| {
                if let Err(send_err) = tx.send(result) {
                    tracing::warn!("Wallet configs watcher channel closed: {}", send_err);
                }
            },
            NotifyConfig::default(),
        )?;

        let parent_dir = Path::new(&file_path)
            .parent()
            .ok_or(WalletConfigsError::ParentDirectory)?;
        watcher.watch(parent_dir, RecursiveMode::NonRecursive)?;

        let wallet_feature_flags_clone = wallet_configs.clone();
        let watched_file_name = file_path.file_name().map(|n| n.to_os_string());
        let debounce_duration = Duration::from_millis(250);

        let watch_task = tokio::spawn(async move {
            // Atomic file saves commonly emit bursts of events (Create/Rename/Modify).
            // We debounce these bursts to avoid reloading (and logging) repeatedly.
            let mut reload_pending = false;
            let reload_sleep = time::sleep(debounce_duration);
            tokio::pin!(reload_sleep);

            loop {
                tokio::select! {
                    biased;
                    maybe_result = rx.recv() => {
                        let Some(result) = maybe_result else {
                            break;
                        };

                        match result {
                            Ok(event) => {
                                if !Self::is_reload_event_kind(&event.kind) {
                                    continue;
                                }

                                let should_reload = watched_file_name
                                    .as_deref()
                                    .map(|name| event.paths.iter().any(|p| p.file_name() == Some(name)))
                                    .unwrap_or(false);

                                if !should_reload {
                                    continue;
                                }

                                reload_pending = true;
                                reload_sleep.as_mut().reset(time::Instant::now() + debounce_duration);
                            }
                            Err(err) => {
                                tracing::error!("Wallet configs watcher error: {}", err);
                            }
                        }
                    }
                    _ = &mut reload_sleep, if reload_pending => {
                        reload_pending = false;

                        match Self::read_flags_from_file_async(&file_path).await {
                            Ok(updated_flags) => {
                                if let Ok(mut write_guard) = wallet_feature_flags_clone.write() {
                                    if *write_guard == updated_flags {
                                        // Avoid noisy log spam when events are emitted without content changes.
                                        continue;
                                    }

                                    *write_guard = updated_flags;
                                    tracing::info!(
                                        "Wallet configs reloaded from {}",
                                        file_path.display()
                                    );
                                }
                            }
                            Err(err) => {
                                tracing::warn!(
                                    "Failed to reload wallet configs from {}: {}. Using last known good configs.",
                                    file_path.display(),
                                    err
                                );
                            }
                        }
                    }
                }
            }
        });

        Ok(Self {
            wallet_configs,
            _watcher: watcher,
            _watch_task: watch_task,
        })
    }

    pub fn get_wallet_configs(&self) -> Result<Value, WalletConfigsError> {
        let guard = self
            .wallet_configs
            .read()
            .map_err(|_| WalletConfigsError::ReadLock("Failed to read wallet configs from lock".to_string()))?;

        Ok(guard.clone())
    }

    // Synchronous read for initial startup
    fn read_flags_from_file_sync(path: &Path) -> Result<Value, WalletConfigsError> {
        let content = std::fs::read_to_string(path)?;
        let configs = serde_json::from_str::<Value>(&content)?;
        Ok(configs)
    }

    // Asynchronous read for the background watcher task
    async fn read_flags_from_file_async(path: &Path) -> Result<Value, WalletConfigsError> {
        let content = tokio::fs::read_to_string(path).await?;
        // For larger JSON payloads, you might want to wrap this next line in spawn_blocking,
        // but for a tiny struct of bools, inline is perfectly fine.
        let configs = serde_json::from_str::<Value>(&content)?;
        Ok(configs)
    }
}

impl Drop for WalletConfigService {
    fn drop(&mut self) {
        self._watch_task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{path::PathBuf, time::Duration};
    use uuid::Uuid;

    fn unique_temp_flags_path() -> PathBuf {
        std::env::temp_dir().join(format!("wallet-configs-{}.json", Uuid::new_v4()))
    }

    fn write_flags_file(path: &Path, content: &str) {
        std::fs::write(path, content).expect("failed to write configs file");
    }

    async fn wait_until<F>(timeout: Duration, mut predicate: F)
    where
        F: FnMut() -> bool,
    {
        let step = Duration::from_millis(50);
        let mut elapsed = Duration::ZERO;

        while elapsed < timeout {
            if predicate() {
                return;
            }
            tokio::time::sleep(step).await;
            elapsed += step;
        }

        panic!("condition not met within {:?}", timeout);
    }

    #[tokio::test]
    async fn new_loads_initial_flags_from_file() {
        let path = unique_temp_flags_path();
        write_flags_file(
            &path,
            r#"{
  "enableTestButtons": false,
  "enableKeystoneHardwareWallet": false,
  "enableHighSecurity": true,
  "enableRemoteNotifications": true,
  "enableSwap": true
}"#,
        );

        let service = WalletConfigService::new(path.clone()).expect("service should initialize");
        let configs = service.get_wallet_configs().unwrap();

        assert!(!configs["enableTestButtons"].as_bool().unwrap());
        assert!(!configs["enableKeystoneHardwareWallet"].as_bool().unwrap());
        assert!(configs["enableHighSecurity"].as_bool().unwrap());
        assert!(configs["enableRemoteNotifications"].as_bool().unwrap());
        assert!(configs["enableSwap"].as_bool().unwrap());

        std::fs::remove_file(path).ok();
    }

    #[tokio::test]
    async fn watcher_reloads_flags_when_file_changes() {
        let path = unique_temp_flags_path();
        write_flags_file(
            &path,
            r#"{
  "enableTestButtons": false,
  "enableKeystoneHardwareWallet": false,
  "enableHighSecurity": true,
  "enableRemoteNotifications": true,
  "enableSwap": true
}"#,
        );

        let service = WalletConfigService::new(path.clone()).expect("service should initialize");

        write_flags_file(
            &path,
            r#"{
  "enableTestButtons": true,
  "enableKeystoneHardwareWallet": true,
  "enableHighSecurity": false,
  "enableRemoteNotifications": false,
  "enableSwap": false
}"#,
        );

        wait_until(Duration::from_secs(3), || {
            let configs = service.get_wallet_configs().unwrap();
            configs["enableTestButtons"].as_bool().unwrap()
                && configs["enableKeystoneHardwareWallet"].as_bool().unwrap()
                && !configs["enableHighSecurity"].as_bool().unwrap()
                && !configs["enableRemoteNotifications"].as_bool().unwrap()
                && !configs["enableSwap"].as_bool().unwrap()
        })
        .await;

        std::fs::remove_file(path).ok();
    }

    #[tokio::test]
    async fn watcher_keeps_last_known_good_flags_when_json_becomes_invalid() {
        let path = unique_temp_flags_path();
        write_flags_file(
            &path,
            r#"{
  "enableTestButtons": false,
  "enableKeystoneHardwareWallet": false,
  "enableHighSecurity": true,
  "enableRemoteNotifications": true,
  "enableSwap": true
}"#,
        );

        let service = WalletConfigService::new(path.clone()).expect("service should initialize");
        let before = service.get_wallet_configs().unwrap();

        write_flags_file(&path, r#"{ invalid json }"#);
        tokio::time::sleep(Duration::from_millis(300)).await;

        let after = service.get_wallet_configs().unwrap();
        assert_eq!(
            before["enableTestButtons"].as_bool().unwrap(),
            after["enableTestButtons"].as_bool().unwrap()
        );
        assert_eq!(
            before["enableKeystoneHardwareWallet"].as_bool().unwrap(),
            after["enableKeystoneHardwareWallet"].as_bool().unwrap()
        );
        assert_eq!(
            before["enableHighSecurity"].as_bool().unwrap(),
            after["enableHighSecurity"].as_bool().unwrap()
        );
        assert_eq!(
            before["enableRemoteNotifications"].as_bool().unwrap(),
            after["enableRemoteNotifications"].as_bool().unwrap()
        );
        assert_eq!(
            before["enableSwap"].as_bool().unwrap(),
            after["enableSwap"].as_bool().unwrap()
        );

        std::fs::remove_file(path).ok();
    }
}

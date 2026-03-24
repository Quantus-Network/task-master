use notify::{Config as NotifyConfig, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};
use tokio::{sync::mpsc, task::JoinHandle};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
pub struct WalletFeatureFlags {
    pub enable_test_buttons: bool,
    pub enable_keystone_hardware_wallet: bool,
    pub enable_high_security: bool,
    pub enable_remote_notifications: bool,
    pub enable_swap: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum WalletFeatureFlagsError {
    #[error("Failed to read wallet feature flags file: {0}")]
    ReadFile(#[from] std::io::Error),
    #[error("Failed to parse wallet feature flags JSON: {0}")]
    ParseJson(#[from] serde_json::Error),
    #[error("Failed to initialize file watcher: {0}")]
    Watcher(#[from] notify::Error),
    #[error("Failed to read wallet feature flags: {0}")]
    ReadLock(String),
}

#[derive(Debug)]
pub struct WalletFeatureFlagsService {
    wallet_feature_flags: Arc<RwLock<WalletFeatureFlags>>,
    _watcher: RecommendedWatcher,
    _watch_task: JoinHandle<()>,
}

impl WalletFeatureFlagsService {
    pub fn new(file_path: impl Into<PathBuf>) -> Result<Self, WalletFeatureFlagsError> {
        let file_path = file_path.into();

        let flags = Self::read_flags_from_file_sync(&file_path)?;
        let wallet_feature_flags = Arc::new(RwLock::new(flags));

        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut watcher = RecommendedWatcher::new(
            move |result| {
                if let Err(send_err) = tx.send(result) {
                    tracing::warn!("Wallet feature flags watcher channel closed: {}", send_err);
                }
            },
            NotifyConfig::default(),
        )?;

        let parent_dir = Path::new(&file_path)
            .parent()
            .unwrap_or(Path::new("wallet_feature_flags"));
        watcher.watch(parent_dir, RecursiveMode::NonRecursive)?;

        let wallet_feature_flags_clone = wallet_feature_flags.clone();

        let watch_task = tokio::spawn(async move {
            while let Some(result) = rx.recv().await {
                match result {
                    Ok(event) => {
                        // This ensures Create, Rename, and Modify events triggered by atomic saves are caught.
                        let should_reload = event.paths.iter().any(|p| p.file_name() == file_path.file_name());

                        if !should_reload {
                            continue;
                        }

                        match Self::read_flags_from_file_async(&file_path).await {
                            Ok(updated_flags) => {
                                if let Ok(mut write_guard) = wallet_feature_flags_clone.write() {
                                    *write_guard = updated_flags;
                                    tracing::info!("Wallet feature flags reloaded from {}", file_path.display());
                                }
                            }
                            Err(err) => {
                                tracing::warn!(
                                    "Failed to reload wallet feature flags from {}: {}. Using last known good flags.",
                                    file_path.display(),
                                    err
                                );
                            }
                        }
                    }
                    Err(err) => {
                        tracing::error!("Wallet feature flags watcher error: {}", err);
                    }
                }
            }
        });

        Ok(Self {
            wallet_feature_flags,
            _watcher: watcher,
            _watch_task: watch_task,
        })
    }

    pub fn get_wallet_feature_flags(&self) -> Result<WalletFeatureFlags, WalletFeatureFlagsError> {
        let guard = self.wallet_feature_flags.read().map_err(|_| {
            WalletFeatureFlagsError::ReadLock("Failed to read wallet feature flags from lock".to_string())
        })?;

        Ok(guard.clone())
    }

    // Synchronous read for initial startup
    fn read_flags_from_file_sync(path: &Path) -> Result<WalletFeatureFlags, WalletFeatureFlagsError> {
        let content = std::fs::read_to_string(path)?;
        let flags = serde_json::from_str::<WalletFeatureFlags>(&content)?;
        Ok(flags)
    }

    // Asynchronous read for the background watcher task
    async fn read_flags_from_file_async(path: &Path) -> Result<WalletFeatureFlags, WalletFeatureFlagsError> {
        let content = tokio::fs::read_to_string(path).await?;
        // For larger JSON payloads, you might want to wrap this next line in spawn_blocking,
        // but for a tiny struct of bools, inline is perfectly fine.
        let flags = serde_json::from_str::<WalletFeatureFlags>(&content)?;
        Ok(flags)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{path::PathBuf, time::Duration};
    use uuid::Uuid;

    fn unique_temp_flags_path() -> PathBuf {
        std::env::temp_dir().join(format!("wallet-feature-flags-{}.json", Uuid::new_v4()))
    }

    fn write_flags_file(path: &Path, content: &str) {
        std::fs::write(path, content).expect("failed to write flags file");
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

        let service = WalletFeatureFlagsService::new(path.clone()).expect("service should initialize");
        let flags = service.get_wallet_feature_flags().unwrap();

        assert!(!flags.enable_test_buttons);
        assert!(!flags.enable_keystone_hardware_wallet);
        assert!(flags.enable_high_security);
        assert!(flags.enable_remote_notifications);
        assert!(flags.enable_swap);

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

        let service = WalletFeatureFlagsService::new(path.clone()).expect("service should initialize");

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
            let flags = service.get_wallet_feature_flags().unwrap();
            flags.enable_test_buttons
                && flags.enable_keystone_hardware_wallet
                && !flags.enable_high_security
                && !flags.enable_remote_notifications
                && !flags.enable_swap
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

        let service = WalletFeatureFlagsService::new(path.clone()).expect("service should initialize");
        let before = service.get_wallet_feature_flags().unwrap();

        write_flags_file(&path, r#"{ invalid json }"#);
        tokio::time::sleep(Duration::from_millis(300)).await;

        let after = service.get_wallet_feature_flags().unwrap();
        assert_eq!(before.enable_test_buttons, after.enable_test_buttons);
        assert_eq!(
            before.enable_keystone_hardware_wallet,
            after.enable_keystone_hardware_wallet
        );
        assert_eq!(before.enable_high_security, after.enable_high_security);
        assert_eq!(before.enable_remote_notifications, after.enable_remote_notifications);
        assert_eq!(before.enable_swap, after.enable_swap);

        std::fs::remove_file(path).ok();
    }
}

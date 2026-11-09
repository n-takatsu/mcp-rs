//! Dynamic Configuration Management
//!
//! Provides runtime configuration switching and reloading capabilities

use crate::config::McpConfig;
use crate::error::Error;
use config::Config;
use std::path::Path;
use std::sync::{Arc, RwLock};
use tokio::sync::watch;
use tracing::{error, info, warn};

/// Dynamic configuration manager
pub struct DynamicConfigManager {
    /// Current configuration
    current_config: Arc<RwLock<McpConfig>>,
    /// Configuration file path
    config_path: Arc<RwLock<Option<String>>>,
    /// Configuration change notifier
    change_sender: watch::Sender<u64>,
    /// Configuration change receiver
    change_receiver: watch::Receiver<u64>,
    /// Configuration version counter
    version: Arc<RwLock<u64>>,
}

impl DynamicConfigManager {
    /// Create a new dynamic configuration manager
    pub fn new(initial_config: McpConfig, config_path: Option<String>) -> Self {
        let (sender, receiver) = watch::channel(0);

        Self {
            current_config: Arc::new(RwLock::new(initial_config)),
            config_path: Arc::new(RwLock::new(config_path)),
            change_sender: sender,
            change_receiver: receiver,
            version: Arc::new(RwLock::new(0)),
        }
    }

    /// Get current configuration
    pub fn get_config(&self) -> McpConfig {
        self.current_config.read().unwrap().clone()
    }

    /// Get configuration change receiver for watching changes
    pub fn get_change_receiver(&self) -> watch::Receiver<u64> {
        self.change_receiver.clone()
    }

    /// Switch to a different configuration file
    pub async fn switch_config_file(&self, new_path: &str) -> Result<(), Error> {
        info!("Switching configuration to: {}", new_path);

        // Validate file exists
        if !Path::new(new_path).exists() {
            return Err(Error::Config(format!(
                "設定ファイルが見つかりません: {}",
                new_path
            )));
        }

        // Load new configuration
        let new_config = self.load_config_from_file(new_path).await?;

        // Update configuration
        {
            let mut config = self.current_config.write().unwrap();
            *config = new_config;
        }

        // Update config path
        {
            let mut path = self.config_path.write().unwrap();
            *path = Some(new_path.to_string());
        }

        // Increment version and notify watchers
        self.notify_config_change().await;

        info!("Configuration switched successfully to: {}", new_path);
        Ok(())
    }

    /// Reload configuration from current file
    pub async fn reload_config(&self) -> Result<(), Error> {
        let config_path = {
            let path = self.config_path.read().unwrap();
            path.clone()
        };

        match config_path {
            Some(path) => {
                info!("Reloading configuration from: {}", path);
                self.switch_config_file(&path).await
            }
            None => {
                warn!("No configuration file path set, loading default config");
                let new_config =
                    McpConfig::load().map_err(|e| crate::error::Error::Internal(e.to_string()))?;

                {
                    let mut config = self.current_config.write().unwrap();
                    *config = new_config;
                }

                self.notify_config_change().await;
                info!("Default configuration reloaded");
                Ok(())
            }
        }
    }

    /// Update configuration directly
    pub async fn update_config(&self, new_config: McpConfig) -> Result<(), Error> {
        {
            let mut config = self.current_config.write().unwrap();
            *config = new_config;
        }

        self.notify_config_change().await;
        info!("Configuration updated directly");
        Ok(())
    }

    /// Get current configuration file path
    pub fn get_config_path(&self) -> Option<String> {
        self.config_path.read().unwrap().clone()
    }

    /// Get current configuration version
    pub fn get_version(&self) -> u64 {
        *self.version.read().unwrap()
    }

    /// Load configuration from file
    async fn load_config_from_file(&self, path: &str) -> Result<McpConfig, Error> {
        // Check if file exists
        if !Path::new(path).exists() {
            return Err(Error::Config(format!(
                "設定ファイルが存在しません: {}",
                path
            )));
        }

        // Use a custom config loader that reads from specific file
        use config::Config;

        let settings = ::config::Config::builder()
            .add_source(
                ::config::Config::try_from(&McpConfig::default())
                    .map_err(|e| crate::error::Error::Internal(e.to_string()))?,
            )
            .add_source(::config::File::with_name(path))
            .build()
            .map_err(|e| crate::error::Error::Internal(e.to_string()))?;

        let mut loaded_config: McpConfig = settings
            .try_deserialize()
            .map_err(|e| crate::error::Error::Internal(e.to_string()))?;

        // Apply environment variable expansion for WordPress config
        if let Some(ref mut wp_config) = loaded_config.handlers.wordpress {
            McpConfig::expand_wordpress_config(wp_config)
                .map_err(|e| crate::error::Error::Config(e.to_string()))?;
            wp_config
                .resolve_encrypted_credentials()
                .map_err(|e| crate::error::Error::Internal(e.to_string()))?;
        }

        Ok(loaded_config)
    }

    /// Notify watchers of configuration change
    async fn notify_config_change(&self) {
        let new_version = {
            let mut version = self.version.write().unwrap();
            *version += 1;
            *version
        };

        if let Err(e) = self.change_sender.send(new_version) {
            error!("Failed to notify configuration change: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::SecureCredentials;
    use std::io::Write;

    #[tokio::test]
    // WORDPRESS_MASTER_PASSWORD環境変数を使う他のテスト（同期テストを含む）と
    // 直列化するため、std::sync::Mutexのガードを意図的にawaitをまたいで
    // 保持する。このテストのランタイム内に同じロックを取る他のタスクは
    // 無いため、デッドロックは起きない。
    #[allow(clippy::await_holding_lock)]
    async fn test_load_config_from_file_resolves_encrypted_credentials() {
        // ホットリロード/設定ファイル切り替え経路も、McpConfig::load()と
        // main.rsのload_config_from_file()と同様にencrypted_credentials
        // を復号してusername/passwordへ反映する必要がある。
        let _env_guard = crate::config::WORDPRESS_MASTER_PASSWORD_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let plaintext =
            SecureCredentials::new("wp-user".to_string(), "wp-app-password".to_string());
        let encrypted = plaintext.encrypt("correct-master-password").unwrap();

        let toml_content = format!(
            r#"
[handlers.wordpress]
url = "https://example.com"

[handlers.wordpress.encrypted_credentials]
username = "{}"
encrypted_password = "{}"
nonce = "{}"
salt = "{}"
"#,
            encrypted.username, encrypted.encrypted_password, encrypted.nonce, encrypted.salt
        );

        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        file.write_all(toml_content.as_bytes()).unwrap();
        let path = file.path().to_str().unwrap().to_string();

        unsafe {
            std::env::set_var("WORDPRESS_MASTER_PASSWORD", "correct-master-password");
        }

        let manager = DynamicConfigManager::new(McpConfig::default(), None);
        let loaded = manager.load_config_from_file(&path).await;

        unsafe {
            std::env::remove_var("WORDPRESS_MASTER_PASSWORD");
        }

        let loaded = loaded.expect("config file should load successfully");
        let wp_config = loaded
            .handlers
            .wordpress
            .expect("wordpress config should be present");
        assert_eq!(wp_config.username, "wp-user");
        assert_eq!(wp_config.password, "wp-app-password");
    }
}

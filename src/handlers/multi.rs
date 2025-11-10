//! Multi-Handler Management System
//!
//! 複数のハンドラーを管理し、動的に追加・削除・切り替えを行うシステム

use super::generic::{ExtendedMcpHandler, HandlerConfig, TargetType};
use crate::mcp::{McpError, McpHandler};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// マルチハンドラーマネージャー
pub struct MultiHandlerManager {
    /// 登録済みハンドラー
    handlers: Arc<RwLock<HashMap<String, Arc<dyn McpHandler>>>>,
    /// ハンドラー設定
    configs: Arc<RwLock<HashMap<String, HandlerConfig>>>,
    /// アクティブハンドラー
    active_handler: Arc<RwLock<Option<String>>>,
    /// ハンドラーファクトリー
    factory: Arc<dyn HandlerFactory>,
}

/// ハンドラーファクトリートレイト
#[async_trait]
pub trait HandlerFactory: Send + Sync {
    async fn create_handler(&self, config: &HandlerConfig)
        -> Result<Arc<dyn McpHandler>, McpError>;
    fn supported_types(&self) -> Vec<TargetType>;
}

impl MultiHandlerManager {
    pub fn new(factory: Arc<dyn HandlerFactory>) -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
            configs: Arc::new(RwLock::new(HashMap::new())),
            active_handler: Arc::new(RwLock::new(None)),
            factory,
        }
    }

    /// ハンドラーを動的に追加
    pub async fn add_handler(&self, id: String, config: HandlerConfig) -> Result<(), McpError> {
        // ハンドラーを作成
        let handler = self.factory.create_handler(&config).await?;

        // 登録
        let mut handlers = self.handlers.write().await;
        let mut configs = self.configs.write().await;

        handlers.insert(id.clone(), handler);
        configs.insert(id.clone(), config);

        // 最初のハンドラーをアクティブに設定
        let mut active = self.active_handler.write().await;
        if active.is_none() {
            *active = Some(id);
        }

        Ok(())
    }

    /// ハンドラーを削除
    pub async fn remove_handler(&self, id: &str) -> Result<(), McpError> {
        let mut handlers = self.handlers.write().await;
        let mut configs = self.configs.write().await;

        handlers.remove(id);
        configs.remove(id);

        // アクティブハンドラーが削除された場合、別のハンドラーを選択
        let mut active = self.active_handler.write().await;
        if active.as_ref() == Some(&id.to_string()) {
            *active = handlers.keys().next().cloned();
        }

        Ok(())
    }

    /// アクティブハンドラーを切り替え
    pub async fn switch_handler(&self, id: &str) -> Result<(), McpError> {
        let handlers = self.handlers.read().await;
        if !handlers.contains_key(id) {
            return Err(McpError::InvalidRequest(format!(
                "Handler '{}' not found",
                id
            )));
        }

        let mut active = self.active_handler.write().await;
        *active = Some(id.to_string());
        Ok(())
    }

    /// 利用可能なハンドラーを一覧取得
    pub async fn list_handlers(&self) -> Vec<HandlerInfo> {
        let handlers = self.handlers.read().await;
        let configs = self.configs.read().await;
        let active = self.active_handler.read().await;

        let mut result = Vec::new();
        for (id, _handler) in handlers.iter() {
            if let Some(config) = configs.get(id) {
                result.push(HandlerInfo {
                    id: id.clone(),
                    name: config.name.clone(),
                    target_type: config.target_type.clone(),
                    is_active: active.as_ref() == Some(id),
                });
            }
        }
        result
    }

    /// アクティブハンドラーを取得
    pub async fn get_active_handler(&self) -> Option<Arc<dyn McpHandler>> {
        let active = self.active_handler.read().await;
        if let Some(id) = active.as_ref() {
            let handlers = self.handlers.read().await;
            handlers.get(id).cloned()
        } else {
            None
        }
    }
}

/// ハンドラー情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandlerInfo {
    pub id: String,
    pub name: String,
    pub target_type: TargetType,
    pub is_active: bool,
}

/// デフォルトハンドラーファクトリー
pub struct DefaultHandlerFactory;

#[async_trait]
impl HandlerFactory for DefaultHandlerFactory {
    async fn create_handler(
        &self,
        config: &HandlerConfig,
    ) -> Result<Arc<dyn McpHandler>, McpError> {
        match &config.target_type {
            TargetType::WordPress => {
                // HandlerConfigからWordPressConfigに変換
                let (username, password) = match &config.connection.auth {
                    crate::handlers::generic::AuthConfig::BasicAuth { username, password } => {
                        (username.clone(), password.clone())
                    }
                    _ => (String::new(), String::new()),
                };

                let wp_config = crate::config::WordPressConfig {
                    url: config.connection.endpoint.clone(),
                    username,
                    password,
                    enabled: Some(true),
                    timeout_seconds: Some(config.connection.timeout_seconds as u64),
                    rate_limit: Some(crate::config::RateLimitConfig::default()),
                    encrypted_credentials: None,
                };
                Ok(Arc::new(crate::handlers::wordpress::WordPressHandler::new(
                    wp_config,
                )))
            }
            TargetType::Database(db_type) => {
                // データベースハンドラーを作成
                self.create_database_handler(db_type, config).await
            }
            TargetType::Filesystem => {
                // ファイルシステムハンドラーを作成
                self.create_filesystem_handler(config).await
            }
            TargetType::CloudService(provider) => {
                // クラウドサービスハンドラーを作成
                self.create_cloud_handler(provider, config).await
            }
            TargetType::WebAPI => {
                // Web APIハンドラーを作成
                self.create_webapi_handler(config).await
            }
            TargetType::MessageQueue => {
                // メッセージキューハンドラーを作成
                self.create_messagequeue_handler(config).await
            }
            TargetType::Custom(custom_type) => Err(McpError::InvalidRequest(format!(
                "Custom handler type '{}' not implemented",
                custom_type
            ))),
        }
    }

    fn supported_types(&self) -> Vec<TargetType> {
        vec![
            TargetType::WordPress,
            TargetType::Database(crate::handlers::generic::DatabaseType::PostgreSQL),
            TargetType::Database(crate::handlers::generic::DatabaseType::MySQL),
            TargetType::Filesystem,
            TargetType::WebAPI,
        ]
    }
}

impl DefaultHandlerFactory {
    async fn create_database_handler(
        &self,
        _db_type: &crate::handlers::generic::DatabaseType,
        _config: &HandlerConfig,
    ) -> Result<Arc<dyn McpHandler>, McpError> {
        // TODO: 実装
        Err(McpError::InvalidRequest(
            "Database handlers not yet implemented".to_string(),
        ))
    }

    async fn create_filesystem_handler(
        &self,
        _config: &HandlerConfig,
    ) -> Result<Arc<dyn McpHandler>, McpError> {
        // TODO: 実装
        Err(McpError::InvalidRequest(
            "Filesystem handlers not yet implemented".to_string(),
        ))
    }

    async fn create_cloud_handler(
        &self,
        _provider: &crate::handlers::generic::CloudProvider,
        _config: &HandlerConfig,
    ) -> Result<Arc<dyn McpHandler>, McpError> {
        // TODO: 実装
        Err(McpError::InvalidRequest(
            "Cloud service handlers not yet implemented".to_string(),
        ))
    }

    async fn create_webapi_handler(
        &self,
        _config: &HandlerConfig,
    ) -> Result<Arc<dyn McpHandler>, McpError> {
        // TODO: 実装
        Err(McpError::InvalidRequest(
            "Web API handlers not yet implemented".to_string(),
        ))
    }

    async fn create_messagequeue_handler(
        &self,
        _config: &HandlerConfig,
    ) -> Result<Arc<dyn McpHandler>, McpError> {
        // TODO: 実装
        Err(McpError::InvalidRequest(
            "Message queue handlers not yet implemented".to_string(),
        ))
    }
}

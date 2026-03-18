//! プラグイン通信ブローカー
//!
//! セキュアコアサーバーとプラグイン間の安全な通信を仲介するブローカーシステム
//! 暗号化、認証、メッセージフィルタリング、レート制限機能を提供

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::{Duration, Instant};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chacha20poly1305::aead::Aead;
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};
use ed25519_dalek::SigningKey;
use ring::rand::{SecureRandom, SystemRandom};
use ring::{digest, hmac};
use x25519_dalek::{PublicKey, StaticSecret};

use crate::error::McpError;
use crate::plugin_isolation::key_exchange::{KeyExchangeConfig, KeyExchangeProtocol};
use crate::plugin_isolation::PluginState;

/// 通信ブローカー
#[derive(Debug)]
pub struct CommunicationBroker {
    /// アクティブな通信チャネル
    active_channels: Arc<RwLock<HashMap<Uuid, CommunicationChannel>>>,
    /// メッセージフィルター
    message_filters: Arc<MessageFilterEngine>,
    /// レート制限管理
    rate_limiters: Arc<RwLock<HashMap<Uuid, RateLimiter>>>,
    /// 暗号化マネージャー（BrokerMessage トランスポート層暗号化）
    encryption_manager: Arc<EncryptionManager>,
    /// 認証マネージャー
    auth_manager: Arc<AuthenticationManager>,
    /// メッセージキュー
    message_queue: Arc<MessageQueue>,
    /// プラグイン間 E2E 鍵交換プロトコル
    ///
    /// プラグイン同士が直接 ECDH 鍵交換を行い [`EncryptedPayload`] で
    /// エンドツーエンド暗号化通信するための専用コンポーネント。
    pub key_exchange_protocol: Arc<KeyExchangeProtocol>,
    /// 設定
    config: BrokerConfig,
}

/// 通信チャネル
#[derive(Debug)]
pub struct CommunicationChannel {
    /// チャネルID
    pub channel_id: Uuid,
    /// プラグインID
    pub plugin_id: Uuid,
    /// チャネルタイプ
    pub channel_type: ChannelType,
    /// 暗号化設定
    pub encryption_config: ChannelEncryption,
    /// 認証情報
    pub auth_info: AuthenticationInfo,
    /// 送信チャネル
    pub sender: mpsc::UnboundedSender<BrokerMessage>,
    /// 受信チャネル
    pub receiver: Arc<Mutex<mpsc::UnboundedReceiver<BrokerMessage>>>,
    /// 統計情報
    pub stats: ChannelStats,
    /// 作成時刻
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// 最終使用時刻
    pub last_used: chrono::DateTime<chrono::Utc>,
}

/// チャネルタイプ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChannelType {
    /// HTTP/HTTPS通信
    Http,
    /// WebSocket通信
    WebSocket,
    /// gRPC通信
    Grpc,
    /// カスタムプロトコル
    Custom(String),
}

/// チャネル暗号化設定
#[derive(Debug, Clone)]
pub struct ChannelEncryption {
    /// 暗号化アルゴリズム
    pub algorithm: EncryptionAlgorithm,
    /// 暗号化キー
    pub key: Vec<u8>,
    /// 初期化ベクトル
    pub iv: Vec<u8>,
    /// 署名キー
    pub signing_key: Vec<u8>,
}

/// 暗号化アルゴリズム
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EncryptionAlgorithm {
    /// AES-GCM-256
    AesGcm256,
    /// ChaCha20-Poly1305
    ChaCha20Poly1305,
    /// AES-CBC-256
    AesCbc256,
}

/// 認証情報
#[derive(Debug, Clone)]
pub struct AuthenticationInfo {
    /// 認証トークン
    pub token: String,
    /// 証明書
    pub certificate: Option<Vec<u8>>,
    /// 公開鍵
    pub public_key: Vec<u8>,
    /// 署名検証用公開鍵
    pub signing_public_key: Vec<u8>,
    /// 鍵識別子
    pub key_id: String,
    /// 有効期限
    pub expires_at: chrono::DateTime<chrono::Utc>,
    /// 権限
    pub permissions: Vec<String>,
}

/// チャネル統計
#[derive(Debug, Default, Clone)]
pub struct ChannelStats {
    /// 送信メッセージ数
    pub messages_sent: u64,
    /// 受信メッセージ数
    pub messages_received: u64,
    /// 送信バイト数
    pub bytes_sent: u64,
    /// 受信バイト数
    pub bytes_received: u64,
    /// エラー数
    pub error_count: u64,
    /// 平均レスポンス時間（ミリ秒）
    pub avg_response_time_ms: f64,
}

/// ブローカーメッセージ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerMessage {
    /// メッセージID
    pub message_id: Uuid,
    /// 送信者プラグインID
    pub source_plugin_id: Uuid,
    /// 宛先プラグインID（None = セキュアコア）
    pub destination_plugin_id: Option<Uuid>,
    /// メッセージタイプ
    pub message_type: MessageType,
    /// ペイロード
    pub payload: Vec<u8>,
    /// 暗号化されているか
    pub encrypted: bool,
    /// 署名
    pub signature: Option<Vec<u8>>,
    /// タイムスタンプ
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// 有効期限
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// メタデータ
    pub metadata: HashMap<String, String>,
}

/// メッセージタイプ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageType {
    /// リクエスト
    Request,
    /// レスポンス
    Response,
    /// 通知
    Notification,
    /// イベント
    Event,
    /// ハートビート
    Heartbeat,
    /// エラー
    Error,
}

/// メッセージフィルターエンジン
#[derive(Debug)]
pub struct MessageFilterEngine {
    /// アクティブフィルター
    filters: Arc<RwLock<Vec<MessageFilter>>>,
    /// ブロックされたメッセージ統計
    blocked_stats: Arc<Mutex<HashMap<String, u64>>>,
}

/// メッセージフィルター
#[derive(Debug, Clone)]
pub struct MessageFilter {
    /// フィルターID
    pub filter_id: String,
    /// フィルタータイプ
    pub filter_type: FilterType,
    /// フィルター条件
    pub conditions: FilterConditions,
    /// アクション
    pub action: FilterAction,
    /// 優先度
    pub priority: u32,
    /// 有効/無効
    pub enabled: bool,
}

/// フィルタータイプ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterType {
    /// コンテンツフィルター
    Content,
    /// サイズフィルター
    Size,
    /// レート制限フィルター
    RateLimit,
    /// セキュリティフィルター
    Security,
    /// 権限フィルター
    Permission,
}

/// フィルター条件
#[derive(Debug, Clone)]
pub struct FilterConditions {
    /// 対象プラグインID（None = 全体）
    pub target_plugin_id: Option<Uuid>,
    /// メッセージタイプ条件
    pub message_types: Option<Vec<MessageType>>,
    /// 最大サイズ（バイト）
    pub max_size_bytes: Option<usize>,
    /// 禁止文字列パターン
    pub blocked_patterns: Vec<String>,
    /// 必要な権限
    pub required_permissions: Vec<String>,
}

/// フィルターアクション
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterAction {
    /// ブロック
    Block,
    /// 警告ログ
    Warn,
    /// サニタイズ
    Sanitize,
    /// 遅延
    Delay(u64), // ミリ秒
}

/// レート制限管理
#[derive(Debug)]
pub struct RateLimiter {
    /// トークンバケット
    tokens: Arc<Mutex<TokenBucket>>,
    /// 設定
    config: RateLimitConfig,
}

/// トークンバケット
#[derive(Debug)]
pub struct TokenBucket {
    /// 現在のトークン数
    tokens: f64,
    /// 最大トークン数
    max_tokens: f64,
    /// 補充レート（トークン/秒）
    refill_rate: f64,
    /// 最終更新時刻
    last_refill: Instant,
}

/// レート制限設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// 最大リクエスト数/秒
    pub max_requests_per_second: f64,
    /// バーストサイズ
    pub burst_size: u32,
    /// ウィンドウサイズ（秒）
    pub window_size_secs: u64,
    /// 有効/無効
    pub enabled: bool,
}

/// 暗号化マネージャー
#[derive(Debug)]
pub struct EncryptionManager {
    /// キー管理
    key_manager: Arc<KeyManager>,
    /// 暗号化エンジン
    encryption_engines: HashMap<EncryptionAlgorithm, Box<dyn EncryptionEngine>>,
}

/// キー管理
#[derive(Debug)]
pub struct KeyManager {
    /// アクティブキー
    active_keys: Arc<RwLock<HashMap<Uuid, EncryptionKeys>>>,
    /// キーローテーション設定
    rotation_config: KeyRotationConfig,
}

/// 暗号化キー
#[derive(Debug, Clone)]
pub struct EncryptionKeys {
    /// プライマリキー
    pub primary_key: Vec<u8>,
    /// セカンダリキー（ローテーション用）
    pub secondary_key: Option<Vec<u8>>,
    /// 署名キー
    pub signing_key: Vec<u8>,
    /// 作成時刻
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// 有効期限
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// キーローテーション設定
#[derive(Debug, Clone)]
pub struct KeyRotationConfig {
    /// ローテーション間隔（時間）
    pub rotation_interval_hours: u64,
    /// 自動ローテーション有効/無効
    pub auto_rotation_enabled: bool,
    /// 重複期間（時間）
    pub overlap_hours: u64,
}

/// 暗号化エンジントレイト
pub trait EncryptionEngine: Send + Sync + std::fmt::Debug {
    fn encrypt(&self, data: &[u8], key: &[u8], iv: &[u8]) -> Result<Vec<u8>, McpError>;
    fn decrypt(&self, data: &[u8], key: &[u8], iv: &[u8]) -> Result<Vec<u8>, McpError>;
    fn generate_key(&self) -> Vec<u8>;
    fn generate_iv(&self) -> Vec<u8>;
}

/// 認証マネージャー
#[derive(Debug)]
pub struct AuthenticationManager {
    /// アクティブトークン
    active_tokens: Arc<RwLock<HashMap<String, TokenInfo>>>,
    /// 証明書ストア
    certificate_store: Arc<CertificateStore>,
    /// 認証設定
    auth_config: AuthenticationConfig,
}

/// トークン情報
#[derive(Debug, Clone)]
pub struct TokenInfo {
    /// トークン値
    pub token: String,
    /// プラグインID
    pub plugin_id: Uuid,
    /// 発行時刻
    pub issued_at: chrono::DateTime<chrono::Utc>,
    /// 有効期限
    pub expires_at: chrono::DateTime<chrono::Utc>,
    /// 権限
    pub permissions: Vec<String>,
    /// リフレッシュ可能か
    pub refreshable: bool,
}

/// 証明書ストア
#[derive(Debug)]
pub struct CertificateStore {
    /// 証明書
    certificates: Arc<RwLock<HashMap<String, CertificateInfo>>>,
}

/// 証明書情報
#[derive(Debug, Clone)]
pub struct CertificateInfo {
    /// 証明書データ
    pub certificate: Vec<u8>,
    /// 公開鍵
    pub public_key: Vec<u8>,
    /// 発行者
    pub issuer: String,
    /// 有効期限
    pub expires_at: chrono::DateTime<chrono::Utc>,
    /// 失効状況
    pub revoked: bool,
}

/// 認証設定
#[derive(Debug, Clone)]
pub struct AuthenticationConfig {
    /// トークン有効期限（秒）
    pub token_lifetime_secs: u64,
    /// 証明書検証有効/無効
    pub certificate_validation_enabled: bool,
    /// mTLS必須
    pub mtls_required: bool,
    /// トークンリフレッシュ有効/無効
    pub token_refresh_enabled: bool,
}

/// メッセージキュー
#[derive(Debug)]
pub struct MessageQueue {
    /// 送信キュー
    outbound_queue: Arc<Mutex<Vec<QueuedMessage>>>,
    /// 受信キュー
    inbound_queue: Arc<Mutex<Vec<QueuedMessage>>>,
    /// 遅延キュー
    delayed_queue: Arc<Mutex<Vec<DelayedMessage>>>,
    /// 設定
    queue_config: QueueConfig,
}

/// キューイングされたメッセージ
#[derive(Debug, Clone)]
pub struct QueuedMessage {
    /// メッセージ
    pub message: BrokerMessage,
    /// 優先度
    pub priority: u8,
    /// キューイング時刻
    pub queued_at: chrono::DateTime<chrono::Utc>,
    /// 再試行回数
    pub retry_count: u32,
}

/// 遅延メッセージ
#[derive(Debug, Clone)]
pub struct DelayedMessage {
    /// メッセージ
    pub message: BrokerMessage,
    /// 送信予定時刻
    pub scheduled_at: chrono::DateTime<chrono::Utc>,
    /// 遅延理由
    pub delay_reason: String,
}

/// キュー設定
#[derive(Debug, Clone)]
pub struct QueueConfig {
    /// 最大キューサイズ
    pub max_queue_size: usize,
    /// 最大再試行回数
    pub max_retry_attempts: u32,
    /// 再試行間隔（秒）
    pub retry_interval_secs: u64,
    /// メッセージTTL（秒）
    pub message_ttl_secs: u64,
}

/// ブローカー設定
#[derive(Debug, Clone)]
pub struct BrokerConfig {
    /// 最大同時チャネル数
    pub max_concurrent_channels: usize,
    /// デフォルトレート制限
    pub default_rate_limit: RateLimitConfig,
    /// 暗号化設定
    pub encryption_config: EncryptionConfig,
    /// 認証設定
    pub auth_config: AuthenticationConfig,
    /// キュー設定
    pub queue_config: QueueConfig,
    /// タイムアウト設定
    pub timeout_config: TimeoutConfig,
}

/// 暗号化設定
#[derive(Debug, Clone)]
pub struct EncryptionConfig {
    /// デフォルト暗号化アルゴリズム
    pub default_algorithm: EncryptionAlgorithm,
    /// キーローテーション設定
    pub key_rotation: KeyRotationConfig,
    /// 暗号化必須
    pub encryption_required: bool,
}

/// タイムアウト設定
#[derive(Debug, Clone)]
pub struct TimeoutConfig {
    /// 接続タイムアウト（秒）
    pub connection_timeout_secs: u64,
    /// リクエストタイムアウト（秒）
    pub request_timeout_secs: u64,
    /// アイドルタイムアウト（秒）
    pub idle_timeout_secs: u64,
}

impl Default for BrokerConfig {
    fn default() -> Self {
        Self {
            max_concurrent_channels: 1000,
            default_rate_limit: RateLimitConfig {
                max_requests_per_second: 100.0,
                burst_size: 10,
                window_size_secs: 60,
                enabled: true,
            },
            encryption_config: EncryptionConfig {
                default_algorithm: EncryptionAlgorithm::AesGcm256,
                key_rotation: KeyRotationConfig {
                    rotation_interval_hours: 24,
                    auto_rotation_enabled: true,
                    overlap_hours: 1,
                },
                encryption_required: true,
            },
            auth_config: AuthenticationConfig {
                token_lifetime_secs: 3600,
                certificate_validation_enabled: true,
                mtls_required: true,
                token_refresh_enabled: true,
            },
            queue_config: QueueConfig {
                max_queue_size: 10000,
                max_retry_attempts: 3,
                retry_interval_secs: 5,
                message_ttl_secs: 300,
            },
            timeout_config: TimeoutConfig {
                connection_timeout_secs: 30,
                request_timeout_secs: 60,
                idle_timeout_secs: 300,
            },
        }
    }
}

impl CommunicationBroker {
    /// 新しい通信ブローカーを作成
    pub async fn new() -> Result<Self, McpError> {
        Self::new_with_config(BrokerConfig::default()).await
    }

    /// 設定付きで通信ブローカーを作成
    pub async fn new_with_config(config: BrokerConfig) -> Result<Self, McpError> {
        info!("Initializing communication broker");

        let message_filters = Arc::new(MessageFilterEngine::new().await?);
        let encryption_manager = Arc::new(EncryptionManager::new(&config.encryption_config).await?);
        let auth_manager = Arc::new(AuthenticationManager::new(config.auth_config.clone()).await?);
        let message_queue = Arc::new(MessageQueue::new(config.queue_config.clone()).await?);
        let kex_config = KeyExchangeConfig {
            key_lifetime_hours: config.encryption_config.key_rotation.rotation_interval_hours,
            grace_period_hours: config.encryption_config.key_rotation.overlap_hours,
            message_ttl_secs: 300,
        };
        let key_exchange_protocol = Arc::new(KeyExchangeProtocol::new_with_config(kex_config));
        if config.encryption_config.key_rotation.auto_rotation_enabled {
            KeyExchangeProtocol::start_auto_rotation(Arc::clone(&key_exchange_protocol));
        }

        Ok(Self {
            active_channels: Arc::new(RwLock::new(HashMap::new())),
            message_filters,
            rate_limiters: Arc::new(RwLock::new(HashMap::new())),
            encryption_manager,
            auth_manager,
            message_queue,
            key_exchange_protocol,
            config,
        })
    }

    /// プラグインの通信チャネルを登録
    pub async fn register_plugin(
        &self,
        plugin_id: Uuid,
        channel_type: ChannelType,
    ) -> Result<Uuid, McpError> {
        info!(
            "Registering communication channel for plugin: {}",
            plugin_id
        );

        // チャネル数制限チェック
        let channels = self.active_channels.read().await;
        if channels.len() >= self.config.max_concurrent_channels {
            return Err(McpError::Plugin(format!(
                "Maximum number of channels reached: {}",
                self.config.max_concurrent_channels
            )));
        }
        drop(channels);

        // 認証情報を生成
        let auth_info = self
            .auth_manager
            .generate_authentication_info(plugin_id)
            .await?;

        // E2E 鍵交換プロトコルにプラグインを登録
        self.key_exchange_protocol
            .register_plugin(plugin_id)
            .await?;

        // 暗号化設定を生成
        let encryption_config = self
            .encryption_manager
            .generate_channel_encryption(plugin_id)
            .await?;

        // メッセージチャネルを作成
        let (sender, receiver) = mpsc::unbounded_channel();

        // チャネルを作成
        let channel_id = Uuid::new_v4();
        let channel = CommunicationChannel {
            channel_id,
            plugin_id,
            channel_type,
            encryption_config,
            auth_info,
            sender,
            receiver: Arc::new(Mutex::new(receiver)),
            stats: ChannelStats::default(),
            created_at: chrono::Utc::now(),
            last_used: chrono::Utc::now(),
        };

        // レート制限を設定
        let rate_limiter = RateLimiter::new(self.config.default_rate_limit.clone());
        let mut limiters = self.rate_limiters.write().await;
        limiters.insert(plugin_id, rate_limiter);
        drop(limiters);

        // チャネルを登録
        let mut channels = self.active_channels.write().await;
        channels.insert(channel_id, channel);

        info!(
            "Communication channel registered: {} -> {}",
            plugin_id, channel_id
        );
        Ok(channel_id)
    }

    /// プラグインの通信チャネルを削除
    pub async fn unregister_plugin(&self, plugin_id: Uuid) -> Result<(), McpError> {
        info!(
            "Unregistering communication channel for plugin: {}",
            plugin_id
        );

        // チャネルを削除
        let mut channels = self.active_channels.write().await;
        let channel_ids: Vec<Uuid> = channels
            .iter()
            .filter(|(_, channel)| channel.plugin_id == plugin_id)
            .map(|(id, _)| *id)
            .collect();

        for channel_id in channel_ids {
            channels.remove(&channel_id);
        }
        drop(channels);

        // レート制限を削除
        let mut limiters = self.rate_limiters.write().await;
        limiters.remove(&plugin_id);

        // 認証情報を削除
        self.auth_manager.revoke_plugin_tokens(plugin_id).await?;

        // E2E 鍵交換プロトコルからプラグインを削除
        self.key_exchange_protocol.unregister_plugin(plugin_id).await;

        info!("Communication channel unregistered: {}", plugin_id);
        Ok(())
    }

    /// メッセージを送信
    pub async fn send_message(&self, message: BrokerMessage) -> Result<(), McpError> {
        debug!(
            "Sending message: {} -> {:?}",
            message.message_id, message.destination_plugin_id
        );

        // メッセージフィルタリング
        if !self.message_filters.filter_message(&message).await? {
            warn!("Message blocked by filter: {}", message.message_id);
            return Err(McpError::SecurityFailure(
                "Message blocked by filter".to_string(),
            ));
        }

        // レート制限チェック
        if !self.check_rate_limit(message.source_plugin_id).await? {
            warn!("Message rate limited: {}", message.message_id);
            return Err(McpError::Plugin("Rate limit exceeded".to_string()));
        }

        // メッセージを暗号化
        let encrypted_message = self.encryption_manager.encrypt_message(&message).await?;

        // メッセージをキューに追加
        self.message_queue
            .enqueue_message(encrypted_message)
            .await?;

        debug!("Message queued for delivery: {}", message.message_id);
        Ok(())
    }

    /// メッセージを受信
    pub async fn receive_message(
        &self,
        plugin_id: Uuid,
    ) -> Result<Option<BrokerMessage>, McpError> {
        // チャネルを取得
        let channels = self.active_channels.read().await;
        let channel_id = channels
            .iter()
            .find(|(_, channel)| channel.plugin_id == plugin_id)
            .map(|(id, _)| *id);
        drop(channels);

        let channel_id = match channel_id {
            Some(id) => id,
            None => return Ok(None),
        };

        // メッセージを受信
        let channels = self.active_channels.read().await;
        if let Some(channel) = channels.get(&channel_id) {
            let mut receiver = channel.receiver.lock().await;
            if let Ok(message) = receiver.try_recv() {
                // メッセージを復号化
                let decrypted_message = self.encryption_manager.decrypt_message(&message).await?;
                return Ok(Some(decrypted_message));
            }
        }

        Ok(None)
    }

    /// レート制限をチェック
    async fn check_rate_limit(&self, plugin_id: Uuid) -> Result<bool, McpError> {
        let limiters = self.rate_limiters.read().await;
        if let Some(limiter) = limiters.get(&plugin_id) {
            limiter.check_rate_limit().await
        } else {
            Ok(true)
        }
    }

    /// チャネル統計を取得
    pub async fn get_channel_stats(
        &self,
        plugin_id: Uuid,
    ) -> Result<Option<ChannelStats>, McpError> {
        let channels = self.active_channels.read().await;
        for channel in channels.values() {
            if channel.plugin_id == plugin_id {
                return Ok(Some(channel.stats.clone()));
            }
        }
        Ok(None)
    }

    /// 通信ブローカーをシャットダウン
    pub async fn shutdown(&self) -> Result<(), McpError> {
        info!("Shutting down communication broker");

        // 全チャネルを閉じる
        let mut channels = self.active_channels.write().await;
        channels.clear();

        // 認証マネージャーをシャットダウン
        self.auth_manager.shutdown().await?;

        // メッセージキューをシャットダウン
        self.message_queue.shutdown().await?;

        info!("Communication broker shutdown completed");
        Ok(())
    }
}

// 他のコンポーネントの実装は省略
// 実際の実装では各コンポーネントの詳細な実装が必要

impl MessageFilterEngine {
    async fn new() -> Result<Self, McpError> {
        Ok(Self {
            filters: Arc::new(RwLock::new(Vec::new())),
            blocked_stats: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    async fn filter_message(&self, _message: &BrokerMessage) -> Result<bool, McpError> {
        // TODO: 実際のフィルタリングロジックを実装
        Ok(true)
    }
}

impl RateLimiter {
    fn new(config: RateLimitConfig) -> Self {
        let token_bucket = TokenBucket {
            tokens: config.burst_size as f64,
            max_tokens: config.burst_size as f64,
            refill_rate: config.max_requests_per_second,
            last_refill: Instant::now(),
        };

        Self {
            tokens: Arc::new(Mutex::new(token_bucket)),
            config,
        }
    }

    async fn check_rate_limit(&self) -> Result<bool, McpError> {
        if !self.config.enabled {
            return Ok(true);
        }

        let mut bucket = self.tokens.lock().await;
        let now = Instant::now();
        let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();

        // トークンを補充
        bucket.tokens = (bucket.tokens + elapsed * bucket.refill_rate).min(bucket.max_tokens);
        bucket.last_refill = now;

        // トークンがあるかチェック
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl EncryptionManager {
    async fn new(config: &EncryptionConfig) -> Result<Self, McpError> {
        Ok(Self {
            key_manager: Arc::new(KeyManager {
                active_keys: Arc::new(RwLock::new(HashMap::new())),
                rotation_config: config.key_rotation.clone(),
            }),
            encryption_engines: HashMap::new(),
        })
    }

    async fn generate_channel_encryption(
        &self,
        plugin_id: Uuid,
    ) -> Result<ChannelEncryption, McpError> {
        let rng = SystemRandom::new();
        let mut key = vec![0u8; 32];
        let mut iv = vec![0u8; 12];
        let mut signing_key = vec![0u8; 32];

        rng.fill(&mut key)
            .map_err(|e| McpError::SecurityFailure(format!("Failed to generate channel key: {e}")))?;
        rng.fill(&mut iv)
            .map_err(|e| McpError::SecurityFailure(format!("Failed to generate channel nonce: {e}")))?;
        rng.fill(&mut signing_key)
            .map_err(|e| McpError::SecurityFailure(format!("Failed to generate signing key: {e}")))?;

        let now = chrono::Utc::now();
        let expires_at = now
            + chrono::Duration::hours(self.key_manager.rotation_config.rotation_interval_hours as i64);
        let mut active_keys = self.key_manager.active_keys.write().await;
        active_keys.insert(
            plugin_id,
            EncryptionKeys {
                primary_key: key.clone(),
                secondary_key: None,
                signing_key: signing_key.clone(),
                created_at: now,
                expires_at,
            },
        );

        Ok(ChannelEncryption {
            algorithm: EncryptionAlgorithm::ChaCha20Poly1305,
            key,
            iv,
            signing_key,
        })
    }

    async fn encrypt_message(&self, message: &BrokerMessage) -> Result<BrokerMessage, McpError> {
        if message.encrypted {
            return Ok(message.clone());
        }

        let keys = self.key_manager.active_keys.read().await;
        let key_material = keys.get(&message.source_plugin_id).ok_or_else(|| {
            McpError::SecurityFailure(format!(
                "No encryption key for source plugin {}",
                message.source_plugin_id
            ))
        })?;

        let mut nonce_bytes = [0u8; 12];
        let nonce_input = digest::digest(
            &digest::SHA256,
            format!("{}:{}", message.message_id, message.timestamp.timestamp_nanos_opt().unwrap_or_default()).as_bytes(),
        );
        nonce_bytes.copy_from_slice(&nonce_input.as_ref()[..12]);

        let cipher = ChaCha20Poly1305::new_from_slice(&key_material.primary_key)
            .map_err(|e| McpError::SecurityFailure(format!("Invalid encryption key length: {e}")))?;

        let ciphertext = cipher
            .encrypt(&Nonce::from(nonce_bytes), message.payload.as_ref())
            .map_err(|e| McpError::SecurityFailure(format!("Message encryption failed: {e}")))?;

        let signature = hmac::sign(
            &hmac::Key::new(hmac::HMAC_SHA256, &key_material.signing_key),
            &ciphertext,
        )
        .as_ref()
        .to_vec();

        let mut encrypted = message.clone();
        encrypted.payload = ciphertext;
        encrypted.encrypted = true;
        encrypted.signature = Some(signature);
        encrypted
            .metadata
            .insert("nonce".to_string(), URL_SAFE_NO_PAD.encode(nonce_bytes));
        encrypted.metadata.insert(
            "algorithm".to_string(),
            "chacha20poly1305+hmac-sha256".to_string(),
        );

        Ok(encrypted)
    }

    async fn decrypt_message(&self, message: &BrokerMessage) -> Result<BrokerMessage, McpError> {
        if !message.encrypted {
            return Ok(message.clone());
        }

        let keys = self.key_manager.active_keys.read().await;
        let key_material = keys.get(&message.source_plugin_id).ok_or_else(|| {
            McpError::SecurityFailure(format!(
                "No decryption key for source plugin {}",
                message.source_plugin_id
            ))
        })?;

        let signature = message.signature.as_ref().ok_or_else(|| {
            McpError::SecurityFailure("Encrypted message missing signature".to_string())
        })?;
        hmac::verify(
            &hmac::Key::new(hmac::HMAC_SHA256, &key_material.signing_key),
            &message.payload,
            signature,
        )
        .map_err(|_| McpError::SecurityFailure("Encrypted message signature verification failed".to_string()))?;

        let nonce_b64 = message.metadata.get("nonce").ok_or_else(|| {
            McpError::SecurityFailure("Encrypted message missing nonce metadata".to_string())
        })?;
        let nonce_raw = URL_SAFE_NO_PAD
            .decode(nonce_b64)
            .map_err(|e| McpError::SecurityFailure(format!("Invalid nonce encoding: {e}")))?;
        if nonce_raw.len() != 12 {
            return Err(McpError::SecurityFailure(
                "Invalid nonce length for ChaCha20-Poly1305".to_string(),
            ));
        }

        let cipher = ChaCha20Poly1305::new_from_slice(&key_material.primary_key)
            .map_err(|e| McpError::SecurityFailure(format!("Invalid decryption key length: {e}")))?;
        let nonce_array: [u8; 12] = nonce_raw
            .as_slice()
            .try_into()
            .map_err(|_| McpError::SecurityFailure("Invalid nonce length for ChaCha20-Poly1305".to_string()))?;
        let plaintext = cipher
            .decrypt(&Nonce::from(nonce_array), message.payload.as_ref())
            .map_err(|e| McpError::SecurityFailure(format!("Message decryption failed: {e}")))?;

        let mut decrypted = message.clone();
        decrypted.payload = plaintext;
        decrypted.encrypted = false;
        Ok(decrypted)
    }
}

impl AuthenticationManager {
    async fn new(_config: AuthenticationConfig) -> Result<Self, McpError> {
        Ok(Self {
            active_tokens: Arc::new(RwLock::new(HashMap::new())),
            certificate_store: Arc::new(CertificateStore {
                certificates: Arc::new(RwLock::new(HashMap::new())),
            }),
            auth_config: _config,
        })
    }

    async fn generate_authentication_info(
        &self,
        plugin_id: Uuid,
    ) -> Result<AuthenticationInfo, McpError> {
        let now = chrono::Utc::now();
        let expires_at = now + chrono::Duration::seconds(self.auth_config.token_lifetime_secs as i64);
        let token = Self::generate_secure_token(plugin_id)?;

        let (ecdh_public_key, signing_public_key) = Self::generate_plugin_key_material()?;
        let key_id = format!("key_{}_{}", plugin_id, now.timestamp());

        // Keep a lightweight certificate entry so peer verification metadata can be referenced.
        {
            let mut certs = self.certificate_store.certificates.write().await;
            certs.insert(
                key_id.clone(),
                CertificateInfo {
                    certificate: Vec::new(),
                    public_key: signing_public_key.clone(),
                    issuer: format!("plugin-auth-manager:{}", plugin_id),
                    expires_at,
                    revoked: false,
                },
            );
        }

        let token_info = TokenInfo {
            token: token.clone(),
            plugin_id,
            issued_at: now,
            expires_at,
            permissions: vec![
                "plugin:communicate".to_string(),
                "plugin:e2e-encryption".to_string(),
            ],
            refreshable: self.auth_config.token_refresh_enabled,
        };

        {
            let mut tokens = self.active_tokens.write().await;
            tokens.insert(token.clone(), token_info);
        }

        Ok(AuthenticationInfo {
            token,
            certificate: None,
            public_key: ecdh_public_key,
            signing_public_key,
            key_id,
            expires_at,
            permissions: vec![
                "plugin:communicate".to_string(),
                "plugin:e2e-encryption".to_string(),
            ],
        })
    }

    async fn revoke_plugin_tokens(&self, plugin_id: Uuid) -> Result<(), McpError> {
        let mut tokens = self.active_tokens.write().await;
        tokens.retain(|_, info| info.plugin_id != plugin_id);

        let mut certs = self.certificate_store.certificates.write().await;
        let plugin_marker = plugin_id.to_string();
        certs.retain(|_, cert| !cert.issuer.contains(&plugin_marker));

        Ok(())
    }

    async fn shutdown(&self) -> Result<(), McpError> {
        self.active_tokens.write().await.clear();
        self.certificate_store.certificates.write().await.clear();
        Ok(())
    }

    fn generate_secure_token(plugin_id: Uuid) -> Result<String, McpError> {
        let rng = SystemRandom::new();
        let mut random = [0u8; 32];
        rng.fill(&mut random)
            .map_err(|e| McpError::SecurityFailure(format!("Failed to generate token entropy: {e}")))?;

        let nonce = URL_SAFE_NO_PAD.encode(random);
        Ok(format!("p_{}_{}", plugin_id.simple(), nonce))
    }

    fn generate_plugin_key_material() -> Result<(Vec<u8>, Vec<u8>), McpError> {
        let rng = SystemRandom::new();

        let mut ecdh_secret_bytes = [0u8; 32];
        rng.fill(&mut ecdh_secret_bytes)
            .map_err(|e| McpError::SecurityFailure(format!("Failed to generate ECDH key: {e}")))?;
        let ecdh_secret = StaticSecret::from(ecdh_secret_bytes);
        let ecdh_public = PublicKey::from(&ecdh_secret).to_bytes().to_vec();

        let mut signing_secret_bytes = [0u8; 32];
        rng.fill(&mut signing_secret_bytes).map_err(|e| {
            McpError::SecurityFailure(format!("Failed to generate signing key material: {e}"))
        })?;
        let signing_key = SigningKey::from_bytes(&signing_secret_bytes);
        let signing_public = signing_key.verifying_key().to_bytes().to_vec();

        Ok((ecdh_public, signing_public))
    }
}

impl MessageQueue {
    async fn new(_config: QueueConfig) -> Result<Self, McpError> {
        Ok(Self {
            outbound_queue: Arc::new(Mutex::new(Vec::new())),
            inbound_queue: Arc::new(Mutex::new(Vec::new())),
            delayed_queue: Arc::new(Mutex::new(Vec::new())),
            queue_config: _config,
        })
    }

    async fn enqueue_message(&self, _message: BrokerMessage) -> Result<(), McpError> {
        // TODO: 実装
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), McpError> {
        // TODO: 実装
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_communication_broker_creation() {
        let broker = CommunicationBroker::new().await;
        assert!(broker.is_ok());
    }

    #[tokio::test]
    async fn test_plugin_registration() {
        let broker = CommunicationBroker::new().await.unwrap();
        let plugin_id = Uuid::new_v4();

        let result = broker.register_plugin(plugin_id, ChannelType::Http).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_rate_limiter() {
        let config = RateLimitConfig {
            max_requests_per_second: 10.0,
            burst_size: 5,
            window_size_secs: 60,
            enabled: true,
        };

        let limiter = RateLimiter::new(config);
        assert!(limiter.config.enabled);
    }

    /// E2E暗号化ラウンドトリップテスト:
    /// ChaCha20-Poly1305 で暗号化したメッセージを復号し、
    /// ペイロードが元のデータと一致することを確認する。
    #[tokio::test]
    async fn test_e2e_encryption_roundtrip() {
        let config = EncryptionConfig {
            default_algorithm: EncryptionAlgorithm::ChaCha20Poly1305,
            key_rotation: KeyRotationConfig {
                rotation_interval_hours: 24,
                auto_rotation_enabled: false,
                overlap_hours: 0,
            },
            encryption_required: true,
        };
        let manager = EncryptionManager::new(&config).await.unwrap();
        let plugin_id = Uuid::new_v4();

        // チャネル鍵を生成 (KeyManager に登録される)
        manager
            .generate_channel_encryption(plugin_id)
            .await
            .unwrap();

        let original_payload = b"Hello, secure world!".to_vec();
        let message = BrokerMessage {
            message_id: Uuid::new_v4(),
            source_plugin_id: plugin_id,
            destination_plugin_id: None,
            message_type: MessageType::Request,
            payload: original_payload.clone(),
            encrypted: false,
            signature: None,
            timestamp: chrono::Utc::now(),
            expires_at: None,
            metadata: HashMap::new(),
        };

        // 暗号化
        let encrypted = manager.encrypt_message(&message).await.unwrap();
        assert!(encrypted.encrypted, "メッセージは暗号化されているべき");
        assert_ne!(
            encrypted.payload, original_payload,
            "暗号文は平文と異なるべき"
        );
        assert!(encrypted.signature.is_some(), "HMAC署名が付与されているべき");
        assert!(
            encrypted.metadata.contains_key("nonce"),
            "nonceメタデータが存在するべき"
        );
        assert_eq!(
            encrypted.metadata.get("algorithm").map(String::as_str),
            Some("chacha20poly1305+hmac-sha256"),
            "アルゴリズムメタデータが正しく設定されているべき"
        );

        // 復号
        let decrypted = manager.decrypt_message(&encrypted).await.unwrap();
        assert!(!decrypted.encrypted, "復号後は encrypted=false になるべき");
        assert_eq!(
            decrypted.payload, original_payload,
            "復号したペイロードが元のデータと一致するべき"
        );
    }

    /// 改ざん検出テスト:
    /// 暗号文のバイトを反転させた場合、HMAC署名検証が失敗し
    /// `decrypt_message` がエラーを返すことを確認する。
    #[tokio::test]
    async fn test_tampered_ciphertext_detection() {
        let config = EncryptionConfig {
            default_algorithm: EncryptionAlgorithm::ChaCha20Poly1305,
            key_rotation: KeyRotationConfig {
                rotation_interval_hours: 24,
                auto_rotation_enabled: false,
                overlap_hours: 0,
            },
            encryption_required: true,
        };
        let manager = EncryptionManager::new(&config).await.unwrap();
        let plugin_id = Uuid::new_v4();

        manager
            .generate_channel_encryption(plugin_id)
            .await
            .unwrap();

        let message = BrokerMessage {
            message_id: Uuid::new_v4(),
            source_plugin_id: plugin_id,
            destination_plugin_id: None,
            message_type: MessageType::Request,
            payload: b"sensitive data".to_vec(),
            encrypted: false,
            signature: None,
            timestamp: chrono::Utc::now(),
            expires_at: None,
            metadata: HashMap::new(),
        };

        let mut encrypted = manager.encrypt_message(&message).await.unwrap();

        // 暗号文の先頭バイトを反転して改ざんを模倣
        if let Some(byte) = encrypted.payload.first_mut() {
            *byte ^= 0xFF;
        }

        // HMAC 署名検証が失敗し復号エラーになるはず
        let result = manager.decrypt_message(&encrypted).await;
        assert!(result.is_err(), "改ざんされた暗号文は復号に失敗するべき");
        let err_str = format!("{:?}", result.unwrap_err());
        assert!(
            err_str.contains("signature verification failed")
                || err_str.contains("SecurityFailure"),
            "エラーは署名検証失敗を示すべき: {err_str}"
        );
    }

    /// nonceユニーク性テスト (リプレイ攻撃対策):
    /// 同じプラグインから送られた異なる2つのメッセージは、
    /// それぞれ一意のnonceを持つことを確認する。
    /// (ChaCha20-Poly1305 は同一鍵+同一nonceの再利用が禁止されているため)
    #[tokio::test]
    async fn test_nonce_uniqueness_prevents_replay() {
        let config = EncryptionConfig {
            default_algorithm: EncryptionAlgorithm::ChaCha20Poly1305,
            key_rotation: KeyRotationConfig {
                rotation_interval_hours: 24,
                auto_rotation_enabled: false,
                overlap_hours: 0,
            },
            encryption_required: true,
        };
        let manager = EncryptionManager::new(&config).await.unwrap();
        let plugin_id = Uuid::new_v4();

        manager
            .generate_channel_encryption(plugin_id)
            .await
            .unwrap();

        let make_message = |payload: &'static [u8]| BrokerMessage {
            message_id: Uuid::new_v4(),
            source_plugin_id: plugin_id,
            destination_plugin_id: None,
            message_type: MessageType::Request,
            payload: payload.to_vec(),
            encrypted: false,
            signature: None,
            timestamp: chrono::Utc::now(),
            expires_at: None,
            metadata: HashMap::new(),
        };

        let msg1 = make_message(b"message one");
        let msg2 = make_message(b"message two");

        let enc1 = manager.encrypt_message(&msg1).await.unwrap();
        let enc2 = manager.encrypt_message(&msg2).await.unwrap();

        let nonce1 = enc1.metadata.get("nonce").expect("nonce1 が存在するべき");
        let nonce2 = enc2.metadata.get("nonce").expect("nonce2 が存在するべき");

        assert_ne!(
            nonce1, nonce2,
            "異なるメッセージIDから生成されたnonceは一意であるべき（リプレイ攻撃対策）"
        );

        // 両メッセージを正常に復号できることも確認
        let dec1 = manager.decrypt_message(&enc1).await.unwrap();
        let dec2 = manager.decrypt_message(&enc2).await.unwrap();
        assert_eq!(dec1.payload, b"message one");
        assert_eq!(dec2.payload, b"message two");
    }
}

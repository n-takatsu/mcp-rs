//! WebSocket Type Definitions

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// WebSocket接続状態
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionState {
    /// 切断
    Disconnected,
    /// 接続中
    Connecting,
    /// 接続完了
    Connected,
    /// 再接続中
    Reconnecting,
    /// エラー
    Error,
    /// クローズ済み
    Closed,
}

/// WebSocketメッセージ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WebSocketMessage {
    /// テキストメッセージ
    Text(String),
    /// バイナリメッセージ
    Binary(Vec<u8>),
    /// Ping
    Ping(Vec<u8>),
    /// Pong
    Pong(Vec<u8>),
    /// 接続クローズ
    Close(Option<CloseFrame>),
}

/// クローズフレーム
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloseFrame {
    /// クローズコード
    pub code: u16,
    /// クローズ理由
    pub reason: String,
}

/// 接続メトリクス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionMetrics {
    /// 接続ID
    pub connection_id: String,
    /// 接続確立時刻
    pub connected_at: DateTime<Utc>,
    /// 最終アクティブ時刻
    pub last_active: DateTime<Utc>,
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

/// ストリーミング設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    /// チャンクサイズ（バイト）
    pub chunk_size: usize,
    /// 最大バッファサイズ（バイト）
    pub max_buffer_size: usize,
    /// 圧縮有効化
    pub compression_enabled: bool,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            chunk_size: 8192,
            max_buffer_size: 1024 * 1024, // 1MB
            compression_enabled: true,
        }
    }
}

/// ストリーミング進捗
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamProgress {
    /// 総バイト数
    pub total_bytes: u64,
    /// 転送済みバイト数
    pub transferred_bytes: u64,
    /// 転送速度（バイト/秒）
    pub transfer_rate: f64,
    /// 残り時間（秒）
    pub estimated_time_remaining: Option<f64>,
}

impl StreamProgress {
    /// 進捗率を計算（0.0-1.0）
    pub fn percentage(&self) -> f64 {
        if self.total_bytes == 0 {
            0.0
        } else {
            self.transferred_bytes as f64 / self.total_bytes as f64
        }
    }
}

/// 接続プール設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    /// 最大接続数
    pub max_connections: usize,
    /// 最小接続数
    pub min_connections: usize,
    /// 接続タイムアウト
    pub connection_timeout: Duration,
    /// アイドルタイムアウト
    pub idle_timeout: Duration,
    /// ヘルスチェック間隔
    pub health_check_interval: Duration,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 100,
            min_connections: 5,
            connection_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(300),
            health_check_interval: Duration::from_secs(30),
        }
    }
}

/// プール統計
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStatistics {
    /// 総接続数
    pub total_connections: usize,
    /// アクティブ接続数
    pub active_connections: usize,
    /// アイドル接続数
    pub idle_connections: usize,
    /// 待機中のリクエスト数
    pub pending_requests: usize,
    /// 総リクエスト数
    pub total_requests: u64,
    /// 失敗したリクエスト数
    pub failed_requests: u64,
    /// 平均待機時間（ミリ秒）
    pub avg_wait_time_ms: f64,
}

/// 接続ヘルスステータス
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// 正常
    Healthy,
    /// 警告
    Warning,
    /// 異常
    Unhealthy,
    /// 不明
    Unknown,
}

/// WebSocketトランスポート設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketConfig {
    /// WebSocketサーバーURL
    pub url: String,
    
    /// サーバーモード（true: サーバー、false: クライアント）
    #[serde(default)]
    pub server_mode: bool,
    
    /// 接続タイムアウト（秒）
    #[serde(default = "default_timeout")]
    pub timeout_seconds: Option<u64>,
    
    /// TLS有効化
    #[serde(default)]
    pub enable_tls: bool,
    
    /// ハートビート間隔（秒、0で無効）
    #[serde(default = "default_heartbeat")]
    pub heartbeat_interval: u64,
    
    /// 最大再接続試行回数
    #[serde(default = "default_max_reconnect")]
    pub max_reconnect_attempts: u32,
    
    /// 再接続遅延（秒）
    #[serde(default = "default_reconnect_delay")]
    pub reconnect_delay: u64,
    
    /// 最大メッセージサイズ（バイト）
    #[serde(default = "default_max_message_size")]
    pub max_message_size: usize,
    
    /// 最大同時接続数（サーバーモード）
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,
    
    /// プール設定
    #[serde(default)]
    pub pool_config: PoolConfig,
    
    /// ストリーム設定
    #[serde(default)]
    pub stream_config: StreamConfig,
}

fn default_timeout() -> Option<u64> {
    Some(30)
}

fn default_heartbeat() -> u64 {
    30
}

fn default_max_reconnect() -> u32 {
    5
}

fn default_reconnect_delay() -> u64 {
    5
}

fn default_max_message_size() -> usize {
    16 * 1024 * 1024 // 16MB
}

fn default_max_connections() -> usize {
    100
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            url: "ws://localhost:8080".to_string(),
            server_mode: false,
            timeout_seconds: default_timeout(),
            enable_tls: false,
            heartbeat_interval: default_heartbeat(),
            max_reconnect_attempts: default_max_reconnect(),
            reconnect_delay: default_reconnect_delay(),
            max_message_size: default_max_message_size(),
            max_connections: default_max_connections(),
            pool_config: PoolConfig::default(),
            stream_config: StreamConfig::default(),
        }
    }
}

/// WebSocketConfig ビルダー
#[derive(Debug, Default)]
pub struct WebSocketConfigBuilder {
    url: Option<String>,
    server_mode: Option<bool>,
    timeout_seconds: Option<Option<u64>>,
    enable_tls: Option<bool>,
    heartbeat_interval: Option<u64>,
    max_reconnect_attempts: Option<u32>,
    reconnect_delay: Option<u64>,
    max_message_size: Option<usize>,
    max_connections: Option<usize>,
    pool_config: Option<PoolConfig>,
    stream_config: Option<StreamConfig>,
}

impl WebSocketConfigBuilder {
    /// 新しいビルダーを作成
    pub fn new() -> Self {
        Self::default()
    }
    
    /// URLを設定
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }
    
    /// サーバーモードを設定
    pub fn server_mode(mut self, enabled: bool) -> Self {
        self.server_mode = Some(enabled);
        self
    }
    
    /// タイムアウトを設定
    pub fn timeout(mut self, seconds: u64) -> Self {
        self.timeout_seconds = Some(Some(seconds));
        self
    }
    
    /// TLSを有効化
    pub fn enable_tls(mut self, enabled: bool) -> Self {
        self.enable_tls = Some(enabled);
        self
    }
    
    /// ハートビート間隔を設定
    pub fn heartbeat_interval(mut self, seconds: u64) -> Self {
        self.heartbeat_interval = Some(seconds);
        self
    }
    
    /// 最大再接続試行回数を設定
    pub fn max_reconnect_attempts(mut self, attempts: u32) -> Self {
        self.max_reconnect_attempts = Some(attempts);
        self
    }
    
    /// 再接続遅延を設定
    pub fn reconnect_delay(mut self, seconds: u64) -> Self {
        self.reconnect_delay = Some(seconds);
        self
    }
    
    /// 最大メッセージサイズを設定
    pub fn max_message_size(mut self, size: usize) -> Self {
        self.max_message_size = Some(size);
        self
    }
    
    /// 最大接続数を設定
    pub fn max_connections(mut self, count: usize) -> Self {
        self.max_connections = Some(count);
        self
    }
    
    /// プール設定を設定
    pub fn pool_config(mut self, config: PoolConfig) -> Self {
        self.pool_config = Some(config);
        self
    }
    
    /// ストリーム設定を設定
    pub fn stream_config(mut self, config: StreamConfig) -> Self {
        self.stream_config = Some(config);
        self
    }
    
    /// ビルド
    pub fn build(self) -> WebSocketConfig {
        let default = WebSocketConfig::default();
        
        WebSocketConfig {
            url: self.url.unwrap_or(default.url),
            server_mode: self.server_mode.unwrap_or(default.server_mode),
            timeout_seconds: self.timeout_seconds.unwrap_or(default.timeout_seconds),
            enable_tls: self.enable_tls.unwrap_or(default.enable_tls),
            heartbeat_interval: self.heartbeat_interval.unwrap_or(default.heartbeat_interval),
            max_reconnect_attempts: self.max_reconnect_attempts.unwrap_or(default.max_reconnect_attempts),
            reconnect_delay: self.reconnect_delay.unwrap_or(default.reconnect_delay),
            max_message_size: self.max_message_size.unwrap_or(default.max_message_size),
            max_connections: self.max_connections.unwrap_or(default.max_connections),
            pool_config: self.pool_config.unwrap_or(default.pool_config),
            stream_config: self.stream_config.unwrap_or(default.stream_config),
        }
    }
}

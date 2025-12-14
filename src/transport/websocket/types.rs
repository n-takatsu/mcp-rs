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
    /// プール設定
    pub pool_config: PoolConfig,
    /// ストリーム設定
    pub stream_config: StreamConfig,
    /// WebSocketサーバーURL
    pub url: String,
    /// TLS有効化
    pub enable_tls: bool,
}

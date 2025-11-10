use std::collections::HashMap;
use std::time::Instant;
use uuid::Uuid;

/// トラフィック分散設定
#[derive(Debug, Clone)]
pub struct TrafficSplit {
    /// カナリア版への割り当て率 (0.0-100.0)
    pub canary_percentage: f32,
    /// 分散基準の設定
    pub criteria: SplitCriteria,
    /// 特定ユーザーグループの設定
    pub user_groups: Vec<UserGroup>,
}

/// トラフィック分散の基準
#[derive(Debug, Clone)]
pub enum SplitCriteria {
    /// ランダム分散（デフォルト）
    Random,
    /// ユーザーIDハッシュベース
    UserIdHash,
    /// IPアドレスベース
    IpAddressHash,
    /// カスタム分散ロジック
    Custom(String),
}

/// ユーザーグループ定義
#[derive(Debug, Clone)]
pub struct UserGroup {
    pub name: String,
    pub users: Vec<String>,
    pub force_canary: bool,
}

/// メトリクス収集器
#[derive(Debug, Clone)]
pub struct MetricsCollector {
    /// 安定版メトリクス
    pub stable_metrics: PolicyMetrics,
    /// カナリア版メトリクス
    pub canary_metrics: PolicyMetrics,
    /// 収集開始時刻
    pub collection_start: Instant,
}

/// ポリシー別メトリクス
#[derive(Debug, Clone)]
pub struct PolicyMetrics {
    /// リクエスト総数
    pub total_requests: u64,
    /// 成功リクエスト数
    pub successful_requests: u64,
    /// エラーリクエスト数
    pub error_requests: u64,
    /// 平均レスポンス時間（ミリ秒）
    pub avg_response_time_ms: f64,
    /// 最大レスポンス時間（ミリ秒）
    pub max_response_time_ms: u64,
    /// カスタムメトリクス
    pub custom_metrics: HashMap<String, f64>,
}

/// デプロイメント状態
#[derive(Debug, Clone, PartialEq)]
pub enum DeploymentState {
    /// 待機中（カナリア展開なし）
    Idle,
    /// 初期検証フェーズ
    Validation,
    /// カナリア展開中
    CanaryActive {
        percentage: f32,
        started_at: Instant,
    },
    /// 段階的拡大中
    Scaling {
        from_percentage: f32,
        to_percentage: f32,
        progress: f32,
    },
    /// 完全展開完了
    FullyDeployed,
    /// ロールバック中
    RollingBack,
    /// 失敗状態
    Failed(String),
}

/// カナリアデプロイメントイベント
#[derive(Debug, Clone)]
pub struct CanaryEvent {
    pub id: Uuid,
    pub timestamp: Instant,
    pub event_type: CanaryEventType,
    pub message: String,
    pub metrics: Option<MetricsSnapshot>,
}

/// カナリアイベントタイプ
#[derive(Debug, Clone)]
pub enum CanaryEventType {
    /// カナリア開始
    CanaryStarted { percentage: f32 },
    /// トラフィック割合変更
    TrafficSplitChanged {
        old_percentage: f32,
        new_percentage: f32,
    },
    /// メトリクス更新
    MetricsUpdated,
    /// 成功基準達成
    SuccessCriteriaMet,
    /// 警告検出
    WarningDetected { warning_type: String },
    /// カナリア成功
    CanarySucceeded,
    /// ロールバック開始
    RollbackInitiated { reason: String },
    /// ロールバック完了
    RollbackCompleted,
}

/// メトリクススナップショット
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub stable_success_rate: f64,
    pub canary_success_rate: f64,
    pub stable_avg_response_time: f64,
    pub canary_avg_response_time: f64,
    pub traffic_split_percentage: f32,
}

/// リクエストコンテキスト
///
/// トラフィック分散の決定に使用される情報
#[derive(Debug, Clone)]
pub struct RequestContext {
    pub request_id: String,
    pub user_id: String,
    pub ip_address: String,
    pub user_agent: Option<String>,
    pub custom_headers: HashMap<String, String>,
}

impl Default for TrafficSplit {
    fn default() -> Self {
        Self {
            canary_percentage: 0.0,
            criteria: SplitCriteria::Random,
            user_groups: Vec::new(),
        }
    }
}

impl Default for PolicyMetrics {
    fn default() -> Self {
        Self {
            total_requests: 0,
            successful_requests: 0,
            error_requests: 0,
            avg_response_time_ms: 0.0,
            max_response_time_ms: 0,
            custom_metrics: HashMap::new(),
        }
    }
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            stable_metrics: PolicyMetrics::default(),
            canary_metrics: PolicyMetrics::default(),
            collection_start: Instant::now(),
        }
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl PolicyMetrics {
    /// 成功率を計算 (0.0-100.0)
    pub fn success_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.successful_requests as f64 / self.total_requests as f64 * 100.0
        }
    }

    /// エラー率を計算 (0.0-100.0)
    pub fn error_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.error_requests as f64 / self.total_requests as f64 * 100.0
        }
    }
}

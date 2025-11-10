use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::types::*;
use crate::error::McpError;
use crate::policy_config::PolicyConfig;

/// カナリアデプロイメント管理システム
///
/// 本システムは「炭鉱のカナリア」の概念を実装し、新しいポリシーを
/// 段階的に展開して安全性を確保します。
#[derive(Debug)]
pub struct CanaryDeploymentManager {
    /// 現在の安定版ポリシー
    stable_policy: Arc<RwLock<PolicyConfig>>,
    /// カナリア版ポリシー（展開中のみ存在）
    canary_policy: Arc<RwLock<Option<PolicyConfig>>>,
    /// トラフィック分散設定
    traffic_split: Arc<RwLock<TrafficSplit>>,
    /// メトリクス収集器
    metrics_collector: Arc<RwLock<MetricsCollector>>,
    /// デプロイメント状態
    deployment_state: Arc<RwLock<DeploymentState>>,
    /// イベント通知チャンネル
    event_sender: broadcast::Sender<CanaryEvent>,
}

impl CanaryDeploymentManager {
    /// 新しいカナリアデプロイメント管理システムを作成
    pub fn new(initial_policy: PolicyConfig) -> Self {
        let (event_sender, _) = broadcast::channel(1000);

        Self {
            stable_policy: Arc::new(RwLock::new(initial_policy)),
            canary_policy: Arc::new(RwLock::new(None)),
            traffic_split: Arc::new(RwLock::new(TrafficSplit::default())),
            metrics_collector: Arc::new(RwLock::new(MetricsCollector::new())),
            deployment_state: Arc::new(RwLock::new(DeploymentState::Idle)),
            event_sender,
        }
    }

    /// カナリアデプロイメントを開始
    ///
    /// # 引数
    /// * `canary_policy` - 展開する新しいポリシー
    /// * `initial_percentage` - 初期のトラフィック割合 (0.0-100.0)
    pub async fn start_canary_deployment(
        &self,
        canary_policy: PolicyConfig,
        initial_percentage: f32,
    ) -> Result<(), McpError> {
        info!("🐤 Starting canary deployment: {}", canary_policy.name);

        // 現在の状態確認
        {
            let state = self.deployment_state.read().unwrap();
            if *state != DeploymentState::Idle {
                return Err(McpError::CanaryDeployment(
                    "Another deployment is already in progress".to_string(),
                ));
            }
        }

        // カナリアポリシーを設定
        {
            let mut canary = self.canary_policy.write().unwrap();
            *canary = Some(canary_policy.clone());
        }

        // トラフィック分散を設定
        {
            let mut split = self.traffic_split.write().unwrap();
            split.canary_percentage = initial_percentage;
        }

        // デプロイメント状態を更新
        {
            let mut state = self.deployment_state.write().unwrap();
            *state = DeploymentState::CanaryActive {
                percentage: initial_percentage,
                started_at: Instant::now(),
            };
        }

        // メトリクス収集をリセット
        {
            let mut metrics = self.metrics_collector.write().unwrap();
            *metrics = MetricsCollector::new();
        }

        // イベント送信
        let event = CanaryEvent {
            id: Uuid::new_v4(),
            timestamp: Instant::now(),
            event_type: CanaryEventType::CanaryStarted {
                percentage: initial_percentage,
            },
            message: format!(
                "🐣 Canary deployment started for '{}' with {}% traffic",
                canary_policy.name, initial_percentage
            ),
            metrics: None,
        };

        self.send_event(event).await;

        info!(
            "✅ Canary deployment started successfully with {}% traffic",
            initial_percentage
        );

        Ok(())
    }

    /// トラフィック分散の決定
    ///
    /// 指定されたコンテキストに基づいて、安定版またはカナリア版の
    /// どちらを使用するかを決定します。
    pub fn should_use_canary(&self, context: &RequestContext) -> bool {
        let split = self.traffic_split.read().unwrap();
        let canary_policy = self.canary_policy.read().unwrap();

        // カナリアポリシーが存在しない場合は安定版を使用
        if canary_policy.is_none() {
            return false;
        }

        // デプロイメント状態確認
        let state = self.deployment_state.read().unwrap();
        match *state {
            DeploymentState::CanaryActive { .. } | DeploymentState::Scaling { .. } => {
                // カナリア展開中のみトラフィック分散を行う
            }
            _ => return false,
        }

        // 強制カナリアユーザーグループの確認
        for group in &split.user_groups {
            if group.force_canary && group.users.contains(&context.user_id) {
                debug!(
                    "User {} is in force-canary group: {}",
                    context.user_id, group.name
                );
                return true;
            }
        }

        // 分散基準に基づく判定
        let hash_value = match split.criteria {
            SplitCriteria::Random => {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};

                let mut hasher = DefaultHasher::new();
                context.request_id.hash(&mut hasher);
                Instant::now().elapsed().as_nanos().hash(&mut hasher);
                hasher.finish()
            }
            SplitCriteria::UserIdHash => {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};

                let mut hasher = DefaultHasher::new();
                context.user_id.hash(&mut hasher);
                hasher.finish()
            }
            SplitCriteria::IpAddressHash => {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};

                let mut hasher = DefaultHasher::new();
                context.ip_address.hash(&mut hasher);
                hasher.finish()
            }
            SplitCriteria::Custom(_) => {
                // カスタムロジックは将来実装
                warn!("Custom split criteria not yet implemented, falling back to random");
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};

                let mut hasher = DefaultHasher::new();
                context.request_id.hash(&mut hasher);
                hasher.finish()
            }
        };

        // ハッシュ値を0-100の範囲に正規化
        let normalized = (hash_value % 10000) as f32 / 100.0;
        let should_use = normalized < split.canary_percentage;

        debug!(
            "Traffic split decision: hash={}, normalized={:.2}%, threshold={:.2}%, use_canary={}",
            hash_value, normalized, split.canary_percentage, should_use
        );

        should_use
    }

    /// カナリア版ポリシーを取得
    pub fn get_canary_policy(&self) -> Option<PolicyConfig> {
        self.canary_policy.read().unwrap().clone()
    }

    /// 安定版ポリシーを取得
    pub fn get_stable_policy(&self) -> PolicyConfig {
        self.stable_policy.read().unwrap().clone()
    }

    /// 現在のデプロイメント状態を取得
    pub fn get_deployment_state(&self) -> DeploymentState {
        self.deployment_state.read().unwrap().clone()
    }

    /// トラフィック分散割合を更新
    pub async fn update_traffic_split(&self, new_percentage: f32) -> Result<(), McpError> {
        if !(0.0..=100.0).contains(&new_percentage) {
            return Err(McpError::InvalidInput(
                "Traffic percentage must be between 0.0 and 100.0".to_string(),
            ));
        }

        let old_percentage = {
            let mut split = self.traffic_split.write().unwrap();
            let old = split.canary_percentage;
            split.canary_percentage = new_percentage;
            old
        };

        // イベント送信
        let event = CanaryEvent {
            id: Uuid::new_v4(),
            timestamp: Instant::now(),
            event_type: CanaryEventType::TrafficSplitChanged {
                old_percentage,
                new_percentage,
            },
            message: format!(
                "🔄 Traffic split updated: {:.1}% → {:.1}%",
                old_percentage, new_percentage
            ),
            metrics: self.get_current_metrics_snapshot(),
        };

        self.send_event(event).await;

        info!(
            "Traffic split updated from {:.1}% to {:.1}%",
            old_percentage, new_percentage
        );

        Ok(())
    }

    /// イベントチャンネルを購読
    pub fn subscribe(&self) -> broadcast::Receiver<CanaryEvent> {
        self.event_sender.subscribe()
    }

    /// メトリクスを記録
    pub fn record_request_metrics(&self, is_canary: bool, success: bool, response_time_ms: u64) {
        let mut collector = self.metrics_collector.write().unwrap();
        let metrics = if is_canary {
            &mut collector.canary_metrics
        } else {
            &mut collector.stable_metrics
        };

        metrics.total_requests += 1;
        if success {
            metrics.successful_requests += 1;
        } else {
            metrics.error_requests += 1;
        }

        // 平均レスポンス時間の更新（移動平均）
        let total = metrics.total_requests as f64;
        metrics.avg_response_time_ms =
            (metrics.avg_response_time_ms * (total - 1.0) + response_time_ms as f64) / total;

        // 最大レスポンス時間の更新
        if response_time_ms > metrics.max_response_time_ms {
            metrics.max_response_time_ms = response_time_ms;
        }

        debug!(
            "Recorded metrics: canary={}, success={}, response_time={}ms",
            is_canary, success, response_time_ms
        );
    }

    /// 現在のメトリクススナップショットを取得
    fn get_current_metrics_snapshot(&self) -> Option<MetricsSnapshot> {
        let collector = self.metrics_collector.read().unwrap();
        let traffic_split = self.traffic_split.read().unwrap();

        let stable_success_rate = if collector.stable_metrics.total_requests > 0 {
            collector.stable_metrics.successful_requests as f64
                / collector.stable_metrics.total_requests as f64
                * 100.0
        } else {
            0.0
        };

        let canary_success_rate = if collector.canary_metrics.total_requests > 0 {
            collector.canary_metrics.successful_requests as f64
                / collector.canary_metrics.total_requests as f64
                * 100.0
        } else {
            0.0
        };

        Some(MetricsSnapshot {
            stable_success_rate,
            canary_success_rate,
            stable_avg_response_time: collector.stable_metrics.avg_response_time_ms,
            canary_avg_response_time: collector.canary_metrics.avg_response_time_ms,
            traffic_split_percentage: traffic_split.canary_percentage,
        })
    }

    /// イベントを送信
    async fn send_event(&self, event: CanaryEvent) {
        if let Err(e) = self.event_sender.send(event) {
            error!("Failed to send canary event: {}", e);
        }
    }

    // テスト用のヘルパーメソッド
    #[cfg(test)]
    pub fn get_traffic_split(&self) -> Arc<RwLock<TrafficSplit>> {
        self.traffic_split.clone()
    }

    #[cfg(test)]
    pub fn get_metrics_collector(&self) -> Arc<RwLock<MetricsCollector>> {
        self.metrics_collector.clone()
    }
}

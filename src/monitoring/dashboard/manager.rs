//! Dashboard Manager Implementation

use super::config::DashboardConfig;
use super::widget::DashboardWidget;
use crate::monitoring::metrics::realtime::RealtimeMetrics;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// ダッシュボードマネージャー
pub struct DashboardManager {
    /// メトリクスストア
    metrics: RealtimeMetrics,
    /// ダッシュボード設定
    config: Arc<RwLock<DashboardConfig>>,
}

impl DashboardManager {
    /// 新しいダッシュボードマネージャーを作成
    pub fn new(metrics: RealtimeMetrics, config: DashboardConfig) -> Self {
        Self {
            metrics,
            config: Arc::new(RwLock::new(config)),
        }
    }

    /// ダッシュボード設定を取得
    pub async fn get_config(&self) -> DashboardConfig {
        self.config.read().await.clone()
    }

    /// ウィジェットを追加
    pub async fn add_widget(&self, widget: DashboardWidget) {
        let mut config = self.config.write().await;
        config.add_widget(widget);
    }

    /// ウィジェットデータを取得
    pub async fn get_widget_data(&self, widget_id: &str) -> Option<WidgetData> {
        let config = self.config.read().await;
        let widget = config.widgets.iter().find(|w| w.id == widget_id)?;

        let latest = self.metrics.get_latest(&widget.metric_type).await?;
        let stats = self.metrics.get_statistics(&widget.metric_type).await?;

        Some(WidgetData {
            widget_id: widget_id.to_string(),
            current_value: latest.value,
            statistics: stats,
            timestamp: latest.timestamp,
        })
    }

    /// 全ウィジェットのデータを取得
    pub async fn get_all_widget_data(&self) -> Vec<WidgetData> {
        let config = self.config.read().await;
        let mut data = Vec::new();

        for widget in &config.widgets {
            if let Some(widget_data) = self.get_widget_data(&widget.id).await {
                data.push(widget_data);
            }
        }

        data
    }
}

/// ウィジェットデータ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetData {
    /// ウィジェットID
    pub widget_id: String,
    /// 現在値
    pub current_value: f64,
    /// 統計情報
    pub statistics: crate::monitoring::metrics::realtime::MetricStatistics,
    /// タイムスタンプ
    pub timestamp: std::time::SystemTime,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitoring::dashboard::widget::{DashboardWidget, WidgetType};
    use crate::monitoring::metrics::MetricPoint;
    use crate::monitoring::MetricType;

    #[tokio::test]
    async fn test_dashboard_manager_creation() {
        let metrics = RealtimeMetrics::new(100);
        let config = DashboardConfig::new("Test Dashboard");
        let manager = DashboardManager::new(metrics, config);

        let retrieved_config = manager.get_config().await;
        assert_eq!(retrieved_config.name, "Test Dashboard");
    }

    #[tokio::test]
    async fn test_add_widget_to_manager() {
        let metrics = RealtimeMetrics::new(100);
        let config = DashboardConfig::new("Test Dashboard");
        let manager = DashboardManager::new(metrics, config);

        manager
            .add_widget(DashboardWidget::new(
                "cpu-1",
                "CPU",
                WidgetType::Gauge,
                MetricType::Cpu,
            ))
            .await;

        let config = manager.get_config().await;
        assert_eq!(config.widgets.len(), 1);
    }

    #[tokio::test]
    async fn test_get_widget_data() {
        let metrics = RealtimeMetrics::new(100);
        metrics
            .add_metric(MetricPoint::new(MetricType::Cpu, 75.0))
            .await;

        let mut config = DashboardConfig::new("Test");
        config.add_widget(DashboardWidget::new(
            "cpu-1",
            "CPU",
            WidgetType::Gauge,
            MetricType::Cpu,
        ));

        let manager = DashboardManager::new(metrics, config);

        let data = manager.get_widget_data("cpu-1").await;
        assert!(data.is_some());
        assert_eq!(data.unwrap().current_value, 75.0);
    }
}

//! Dashboard Widget Types

use crate::monitoring::MetricType;
use serde::{Deserialize, Serialize};

/// ダッシュボードウィジェット
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardWidget {
    /// ウィジェットID
    pub id: String,
    /// ウィジェット名
    pub name: String,
    /// ウィジェットタイプ
    pub widget_type: WidgetType,
    /// 監視するメトリクス
    pub metric_type: MetricType,
    /// 更新間隔（秒）
    pub refresh_interval: u64,
}

/// ウィジェットタイプ
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WidgetType {
    /// ラインチャート
    LineChart,
    /// バーチャート
    BarChart,
    /// ゲージ
    Gauge,
    /// 数値表示
    Value,
    /// テーブル
    Table,
}

impl DashboardWidget {
    /// 新しいウィジェットを作成
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        widget_type: WidgetType,
        metric_type: MetricType,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            widget_type,
            metric_type,
            refresh_interval: 5, // デフォルト5秒
        }
    }

    /// 更新間隔を設定
    pub fn with_refresh_interval(mut self, seconds: u64) -> Self {
        self.refresh_interval = seconds;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_widget_creation() {
        let widget = DashboardWidget::new("cpu-1", "CPU Usage", WidgetType::Gauge, MetricType::Cpu);

        assert_eq!(widget.id, "cpu-1");
        assert_eq!(widget.name, "CPU Usage");
        assert_eq!(widget.widget_type, WidgetType::Gauge);
    }

    #[test]
    fn test_widget_with_refresh_interval() {
        let widget = DashboardWidget::new("cpu-1", "CPU Usage", WidgetType::Gauge, MetricType::Cpu)
            .with_refresh_interval(10);

        assert_eq!(widget.refresh_interval, 10);
    }
}

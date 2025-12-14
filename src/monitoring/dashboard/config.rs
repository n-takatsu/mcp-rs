//! Dashboard Configuration

use super::widget::DashboardWidget;
use serde::{Deserialize, Serialize};

/// ダッシュボード設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardConfig {
    /// ダッシュボード名
    pub name: String,
    /// ウィジェットリスト
    pub widgets: Vec<DashboardWidget>,
}

impl DashboardConfig {
    /// 新しいダッシュボード設定を作成
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            widgets: Vec::new(),
        }
    }

    /// ウィジェットを追加
    pub fn add_widget(&mut self, widget: DashboardWidget) {
        self.widgets.push(widget);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitoring::dashboard::widget::{DashboardWidget, WidgetType};
    use crate::monitoring::MetricType;

    #[test]
    fn test_dashboard_config() {
        let mut config = DashboardConfig::new("Main Dashboard");
        config.add_widget(DashboardWidget::new(
            "cpu-1",
            "CPU",
            WidgetType::LineChart,
            MetricType::Cpu,
        ));

        assert_eq!(config.widgets.len(), 1);
        assert_eq!(config.name, "Main Dashboard");
    }
}

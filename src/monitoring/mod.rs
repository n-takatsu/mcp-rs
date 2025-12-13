//! リアルタイム監視分析システム
//!
//! このモジュールは、システムメトリクスの収集、リアルタイム監視、
//! アラート機能を提供します。

pub mod alerts;
pub mod collector;
pub mod dashboard;
pub mod detector;
pub mod metrics;

pub use alerts::{Alert, AlertLevel, AlertManager};
pub use collector::MetricsCollector;
pub use dashboard::DashboardManager;
pub use detector::AnomalyDetector;
pub use metrics::{MetricType, MetricValue, SystemMetrics};

//! Dashboard Module
//!
//! WebSocketベースのリアルタイムダッシュボード

mod config;
mod manager;
mod widget;

pub use config::DashboardConfig;
pub use manager::{DashboardManager, WidgetData};
pub use widget::{DashboardWidget, WidgetType};

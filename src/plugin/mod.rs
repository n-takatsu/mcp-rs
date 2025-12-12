//! プラグイン管理モジュール
//!
//! プラグイン隔離システムの中核機能を提供します。
//!
//! # 機能
//!
//! - **Docker コンテナベース隔離**: 各プラグインを独立したコンテナで実行
//! - **リソース制限と監視**: CPU、メモリ、ディスクI/Oの制限と監視
//! - **ネットワークポリシー制御**: プラグイン間通信とネットワークアクセスの制御

pub mod isolation;
pub mod manager;
pub mod resource;

pub use isolation::{IsolationConfig, IsolationEnvironment, IsolationLevel};
pub use manager::{Plugin, PluginManager, PluginState, PluginStatus};
pub use resource::{ResourceLimits, ResourceMonitor, ResourceUsage};

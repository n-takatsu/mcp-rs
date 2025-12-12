//! ポリシー管理モジュール
//!
//! 動的ポリシー更新システムの中核機能を提供

pub mod dynamic_updater;
pub mod hot_reload;
pub mod rollback;
pub mod version_control;

pub use dynamic_updater::{DynamicPolicyUpdater, PolicyUpdateEvent, UpdateConfig};
pub use hot_reload::{HotReloadManager, ReloadStrategy};
pub use rollback::{RollbackManager, RollbackPoint};
pub use version_control::{PolicyVersion, VersionManager};

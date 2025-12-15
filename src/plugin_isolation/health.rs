//! Plugin Manager Health
//!
//! プラグインマネージャーのヘルス情報

use serde::{Deserialize, Serialize};

/// プラグインマネージャーのヘルス状態
#[derive(Debug, Serialize, Deserialize)]
pub struct PluginManagerHealth {
    /// 総プラグイン数
    pub total_plugins: usize,
    /// 実行中プラグイン数
    pub running_plugins: usize,
    /// エラー状態プラグイン数
    pub error_plugins: usize,
    /// 隔離状態プラグイン数
    pub quarantined_plugins: usize,
    /// システム全体の健全性
    pub system_health: String,
}

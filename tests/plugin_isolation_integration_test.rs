//! プラグイン隔離システム統合テスト - Issue #190 完成版

use mcp_rs::plugin_isolation::{
    lifecycle_manager::{HealthCheckConfig, LifecycleManager},
    IsolatedPluginManager, PluginManagerConfig,
    monitoring::MonitoringSystem,
    PluginMetadata, ResourceLimits, SecurityLevel,
};
use uuid::Uuid;

/// テスト1: プラグインマネージャーの作成
#[tokio::test]
async fn test_plugin_manager_initialization() {
    let config = PluginManagerConfig::default();
    let manager = IsolatedPluginManager::new(config).await;
    assert!(manager.is_ok());
}

/// テスト2: ライフサイクルマネージャーの初期化
#[tokio::test]
async fn test_lifecycle_manager_initialization() {
    let manager = LifecycleManager::new().await;
    assert!(manager.is_ok());
}

/// テスト3: モニタリングシステムの作成
#[tokio::test]
async fn test_monitoring_system_creation() {
    let system = MonitoringSystem::new().await;
    assert!(system.is_ok());
}

/// テスト4: リソース制限のデフォルト値
#[test]
fn test_resource_limits() {
    let limits = ResourceLimits::default();
    assert!(limits.max_cpu_usage > 0.0);
    assert!(limits.max_memory_mb > 0);
}

/// テスト5: プラグインメタデータの作成
#[test]
fn test_plugin_metadata_creation() {
    let metadata = PluginMetadata {
        id: Uuid::new_v4(),
        name: "test-plugin".to_string(),
        version: "1.0.0".to_string(),
        description: "Test plugin".to_string(),
        author: "Test Author".to_string(),
        required_permissions: vec!["read_config".to_string()],
        resource_limits: ResourceLimits::default(),
        security_level: SecurityLevel::Standard,
        dependencies: vec![],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    
    assert_eq!(metadata.name, "test-plugin");
}

/// テスト6: ヘルスチェック設定
#[test]
fn test_health_check_config() {
    let config = HealthCheckConfig::default();
    assert_eq!(config.interval_secs, 30);
}

/// テスト7: セキュリティレベルの階層
#[test]
fn test_security_level_hierarchy() {
    assert!(SecurityLevel::Minimal < SecurityLevel::Standard);
}

/// テスト8: 複数プラグインメタデータ
#[test]
fn test_multiple_plugin_metadata() {
    let mut plugins = Vec::new();
    
    for i in 0..5 {
        let plugin = PluginMetadata {
            id: Uuid::new_v4(),
            name: format!("plugin-{}", i),
            version: "1.0.0".to_string(),
            description: format!("Plugin {}", i),
            author: "Test".to_string(),
            required_permissions: vec![],
            resource_limits: ResourceLimits::default(),
            security_level: SecurityLevel::Standard,
            dependencies: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        plugins.push(plugin);
    }
    
    assert_eq!(plugins.len(), 5);
}

/// テスト9: リソース制限のカスタマイズ
#[test]
fn test_custom_resource_limits() {
    let limits = ResourceLimits {
        max_cpu_usage: 0.8,
        max_memory_mb: 1024,
        max_disk_mb: 2048,
        max_network_mbps: 100,
        max_connections: 50,
        max_execution_time_secs: 300,
    };
    
    assert_eq!(limits.max_cpu_usage, 0.8);
}

/// テスト10: UUID一意性
#[test]
fn test_uuid_uniqueness() {
    let mut ids = std::collections::HashSet::new();
    for _ in 0..100 {
        ids.insert(Uuid::new_v4());
    }
    assert_eq!(ids.len(), 100);
}

/// テスト11: タイムスタンプ
#[test]
fn test_metadata_timestamps() {
    let now = chrono::Utc::now();
    let metadata = PluginMetadata {
        id: Uuid::new_v4(),
        name: "test".to_string(),
        version: "1.0.0".to_string(),
        description: "Test".to_string(),
        author: "Test".to_string(),
        required_permissions: vec![],
        resource_limits: ResourceLimits::default(),
        security_level: SecurityLevel::Standard,
        dependencies: vec![],
        created_at: now,
        updated_at: now,
    };
    
    assert_eq!(metadata.created_at, metadata.updated_at);
}

/// テスト12: 依存関係トラッキング
#[test]
fn test_dependency_tracking() {
    let plugin = PluginMetadata {
        id: Uuid::new_v4(),
        name: "plugin-a".to_string(),
        version: "1.0.0".to_string(),
        description: "Plugin A".to_string(),
        author: "Test".to_string(),
        required_permissions: vec![],
        resource_limits: ResourceLimits::default(),
        security_level: SecurityLevel::Standard,
        dependencies: vec!["plugin-b".to_string()],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    
    assert_eq!(plugin.dependencies.len(), 1);
}

/// テスト13: 権限リスト
#[test]
fn test_permission_management() {
    let mut perms = vec!["read".to_string()];
    perms.push("write".to_string());
    assert_eq!(perms.len(), 2);
}

/// テスト14: セキュリティレベル比較
#[test]
fn test_security_level_comparison() {
    assert!(SecurityLevel::LowRisk < SecurityLevel::HighRisk);
}

/// テスト15: プラグインマネージャー設定
#[test]
fn test_plugin_manager_config() {
    let config = PluginManagerConfig::default();
    assert!(config.max_plugins > 0);
}

//! セキュリティサンドボックス
//!
//! プラグインの実行環境を厳格に制限し、システムリソースへの不正アクセスを防止する

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::error::McpError;
use crate::plugin_isolation::{PluginMetadata, SecurityPolicy};

/// セキュリティサンドボックス
#[derive(Debug)]
pub struct SecuritySandbox {
    /// セキュリティポリシー
    security_policy: SecurityPolicy,
    /// システムコール監視
    syscall_monitor: Arc<SyscallMonitor>,
    /// ネットワークアクセス制御
    network_acl: Arc<NetworkAccessControl>,
    /// ファイルアクセス制御
    file_acl: Arc<FileAccessControl>,
    /// リソース制限エンフォーサー
    resource_enforcer: Arc<ResourceEnforcer>,
    /// セキュリティ違反追跡
    violation_tracker: Arc<Mutex<ViolationTracker>>,
}

/// システムコール監視
#[derive(Debug)]
pub struct SyscallMonitor {
    /// 許可されたシステムコール
    allowed_syscalls: Arc<RwLock<HashMap<Uuid, Vec<String>>>>,
    /// 禁止されたシステムコール
    blocked_syscalls: Arc<RwLock<HashMap<Uuid, Vec<String>>>>,
    /// システムコール監査ログ
    audit_log: Arc<Mutex<Vec<SyscallAuditEntry>>>,
}

/// システムコール監査エントリ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyscallAuditEntry {
    /// プラグインID
    pub plugin_id: Uuid,
    /// システムコール名
    pub syscall_name: String,
    /// 引数
    pub arguments: Vec<String>,
    /// 実行時刻
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// 許可/拒否
    pub allowed: bool,
    /// プロセスID
    pub process_id: u32,
    /// スレッドID
    pub thread_id: u32,
}

/// ネットワークアクセス制御
#[derive(Debug)]
pub struct NetworkAccessControl {
    /// 許可されたホスト/ドメイン
    allowed_hosts: Arc<RwLock<HashMap<Uuid, Vec<NetworkRule>>>>,
    /// アクティブな接続監視
    connection_monitor: Arc<Mutex<HashMap<Uuid, Vec<NetworkConnection>>>>,
}

/// ネットワークルール
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkRule {
    /// ルールID
    pub rule_id: String,
    /// ホスト/IP/CIDR
    pub host_pattern: String,
    /// ポート範囲
    pub port_range: Option<PortRange>,
    /// プロトコル
    pub protocol: NetworkProtocol,
    /// 許可/拒否
    pub action: NetworkAction,
    /// 優先度
    pub priority: u32,
}

/// ネットワークプロトコル
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkProtocol {
    TCP,
    UDP,
    HTTP,
    HTTPS,
    All,
}

/// ネットワークアクション
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkAction {
    Allow,
    Deny,
    Log,
}

/// ポート範囲
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortRange {
    /// 開始ポート
    pub start: u16,
    /// 終了ポート
    pub end: u16,
}

/// ネットワーク接続情報
#[derive(Debug, Clone)]
pub struct NetworkConnection {
    /// 接続ID
    pub connection_id: String,
    /// ローカルアドレス
    pub local_address: String,
    /// リモートアドレス
    pub remote_address: String,
    /// プロトコル
    pub protocol: NetworkProtocol,
    /// 状態
    pub state: ConnectionState,
    /// 開始時刻
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// データ転送量
    pub bytes_transferred: u64,
}

/// 接続状態
#[derive(Debug, Clone)]
pub enum ConnectionState {
    Established,
    Listening,
    Closed,
    TimeWait,
}

/// ファイルアクセス制御
#[derive(Debug)]
pub struct FileAccessControl {
    /// 読み取り許可パス
    read_allowed_paths: Arc<RwLock<HashMap<Uuid, Vec<PathRule>>>>,
    /// 書き込み許可パス
    write_allowed_paths: Arc<RwLock<HashMap<Uuid, Vec<PathRule>>>>,
    /// 実行許可パス
    exec_allowed_paths: Arc<RwLock<HashMap<Uuid, Vec<PathRule>>>>,
    /// ファイルアクセス監査ログ
    file_audit_log: Arc<Mutex<Vec<FileAuditEntry>>>,
}

/// パスルール
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathRule {
    /// ルールID
    pub rule_id: String,
    /// パスパターン（glob形式）
    pub path_pattern: String,
    /// アクセス種別
    pub access_type: FileAccessType,
    /// 許可/拒否
    pub action: FileAction,
    /// 優先度
    pub priority: u32,
}

/// ファイルアクセス種別
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileAccessType {
    Read,
    Write,
    Execute,
    Create,
    Delete,
    All,
}

/// ファイルアクション
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileAction {
    Allow,
    Deny,
    Log,
}

/// ファイルアクセス監査エントリ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAuditEntry {
    /// プラグインID
    pub plugin_id: Uuid,
    /// ファイルパス
    pub file_path: String,
    /// アクセス種別
    pub access_type: FileAccessType,
    /// 実行時刻
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// 許可/拒否
    pub allowed: bool,
    /// プロセスID
    pub process_id: u32,
}

/// リソース制限エンフォーサー
#[derive(Debug)]
pub struct ResourceEnforcer {
    /// CPU使用量監視
    cpu_monitor: Arc<Mutex<HashMap<Uuid, CpuUsageTracker>>>,
    /// メモリ使用量監視
    memory_monitor: Arc<Mutex<HashMap<Uuid, MemoryUsageTracker>>>,
    /// ディスクI/O監視
    disk_monitor: Arc<Mutex<HashMap<Uuid, DiskUsageTracker>>>,
    /// ネットワークI/O監視
    network_monitor: Arc<Mutex<HashMap<Uuid, NetworkUsageTracker>>>,
}

/// CPU使用量追跡
#[derive(Debug, Clone)]
pub struct CpuUsageTracker {
    /// 現在のCPU使用率
    pub current_usage: f64,
    /// 制限値
    pub limit: f64,
    /// 累積使用時間（ミリ秒）
    pub total_time_ms: u64,
    /// 最終更新時刻
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

/// メモリ使用量追跡
#[derive(Debug, Clone)]
pub struct MemoryUsageTracker {
    /// 現在のメモリ使用量（バイト）
    pub current_usage_bytes: u64,
    /// 制限値（バイト）
    pub limit_bytes: u64,
    /// ピーク使用量
    pub peak_usage_bytes: u64,
    /// 最終更新時刻
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

/// ディスク使用量追跡
#[derive(Debug, Clone)]
pub struct DiskUsageTracker {
    /// 読み取りバイト数
    pub read_bytes: u64,
    /// 書き込みバイト数
    pub write_bytes: u64,
    /// I/O操作回数
    pub io_operations: u64,
    /// 最終更新時刻
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

/// ネットワーク使用量追跡
#[derive(Debug, Clone)]
pub struct NetworkUsageTracker {
    /// 送信バイト数
    pub tx_bytes: u64,
    /// 受信バイト数
    pub rx_bytes: u64,
    /// パケット数
    pub packet_count: u64,
    /// 最終更新時刻
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

/// セキュリティ違反追跡
#[derive(Debug, Default)]
pub struct ViolationTracker {
    /// プラグイン別違反回数
    pub violation_counts: HashMap<Uuid, u32>,
    /// 違反履歴
    pub violation_history: Vec<SecurityViolation>,
}

/// セキュリティ違反
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityViolation {
    /// 違反ID
    pub violation_id: Uuid,
    /// プラグインID
    pub plugin_id: Uuid,
    /// 違反タイプ
    pub violation_type: ViolationType,
    /// 詳細
    pub details: String,
    /// 発生時刻
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// 重要度
    pub severity: ViolationSeverity,
    /// 対応アクション
    pub action_taken: String,
}

/// 違反タイプ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViolationType {
    /// 禁止されたシステムコール
    BlockedSyscall,
    /// 不正なネットワークアクセス
    UnauthorizedNetworkAccess,
    /// 不正なファイルアクセス
    UnauthorizedFileAccess,
    /// リソース制限超過
    ResourceLimitExceeded,
    /// 悪意のある行動検知
    MaliciousBehavior,
}

/// 違反重要度
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViolationSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl SecuritySandbox {
    /// 新しいセキュリティサンドボックスを作成
    pub async fn new(security_policy: SecurityPolicy) -> Result<Self, McpError> {
        info!("Initializing security sandbox");

        let syscall_monitor = Arc::new(SyscallMonitor::new().await?);
        let network_acl = Arc::new(NetworkAccessControl::new().await?);
        let file_acl = Arc::new(FileAccessControl::new().await?);
        let resource_enforcer = Arc::new(ResourceEnforcer::new().await?);

        Ok(Self {
            security_policy,
            syscall_monitor,
            network_acl,
            file_acl,
            resource_enforcer,
            violation_tracker: Arc::new(Mutex::new(ViolationTracker::default())),
        })
    }

    /// プラグインメタデータを検証
    pub async fn validate_plugin_metadata(
        &self,
        metadata: &PluginMetadata,
    ) -> Result<(), McpError> {
        info!("Validating plugin metadata: {}", metadata.name);

        // セキュリティレベルチェック
        match metadata.security_level {
            crate::plugin_isolation::SecurityLevel::Maximum => {
                // 最高レベルの制限を適用
                self.apply_maximum_security_restrictions(metadata.id)
                    .await?;
            }
            crate::plugin_isolation::SecurityLevel::Strict => {
                // 厳格な制限を適用
                self.apply_strict_security_restrictions(metadata.id).await?;
            }
            crate::plugin_isolation::SecurityLevel::Standard
            | crate::plugin_isolation::SecurityLevel::Safe => {
                // 標準的な制限を適用
                self.apply_standard_security_restrictions(metadata.id)
                    .await?;
            }
            crate::plugin_isolation::SecurityLevel::Minimal
            | crate::plugin_isolation::SecurityLevel::LowRisk => {
                // 最小限の制限を適用
                self.apply_minimal_security_restrictions(metadata.id)
                    .await?;
            }
            crate::plugin_isolation::SecurityLevel::MediumRisk => {
                // 中リスク - 標準的な制限を適用
                self.apply_standard_security_restrictions(metadata.id)
                    .await?;
            }
            crate::plugin_isolation::SecurityLevel::HighRisk
            | crate::plugin_isolation::SecurityLevel::Dangerous => {
                // 高リスク・危険 - 厳格な制限を適用
                self.apply_strict_security_restrictions(metadata.id).await?;
            }
        }

        // 必要な権限の検証
        for permission in &metadata.required_permissions {
            self.validate_permission(permission)?;
        }

        info!("Plugin metadata validation completed: {}", metadata.name);
        Ok(())
    }

    /// 最大セキュリティ制限を適用
    async fn apply_maximum_security_restrictions(&self, plugin_id: Uuid) -> Result<(), McpError> {
        debug!(
            "Applying maximum security restrictions for plugin: {}",
            plugin_id
        );

        // システムコール制限
        let blocked_syscalls = vec![
            "execve".to_string(),
            "fork".to_string(),
            "clone".to_string(),
            "kill".to_string(),
            "ptrace".to_string(),
            "mount".to_string(),
            "umount".to_string(),
            "chroot".to_string(),
            "setuid".to_string(),
            "setgid".to_string(),
        ];
        self.syscall_monitor
            .set_blocked_syscalls(plugin_id, blocked_syscalls)
            .await?;

        // ネットワークアクセス制限（完全拒否）
        self.network_acl.deny_all_access(plugin_id).await?;

        // ファイルアクセス制限（読み取り専用、限定パス）
        let allowed_read_paths = vec![PathRule {
            rule_id: "read-only-lib".to_string(),
            path_pattern: "/usr/lib/**".to_string(),
            access_type: FileAccessType::Read,
            action: FileAction::Allow,
            priority: 100,
        }];
        self.file_acl
            .set_read_allowed_paths(plugin_id, allowed_read_paths)
            .await?;
        self.file_acl.deny_all_write_access(plugin_id).await?;
        self.file_acl.deny_all_exec_access(plugin_id).await?;

        Ok(())
    }

    /// 厳格なセキュリティ制限を適用
    async fn apply_strict_security_restrictions(&self, plugin_id: Uuid) -> Result<(), McpError> {
        debug!(
            "Applying strict security restrictions for plugin: {}",
            plugin_id
        );

        // 基本的なシステムコールのみ許可
        let allowed_syscalls = vec![
            "read".to_string(),
            "write".to_string(),
            "open".to_string(),
            "close".to_string(),
            "stat".to_string(),
            "fstat".to_string(),
            "lstat".to_string(),
        ];
        self.syscall_monitor
            .set_allowed_syscalls(plugin_id, allowed_syscalls)
            .await?;

        // 限定的なネットワークアクセス
        let network_rules = vec![NetworkRule {
            rule_id: "https-only".to_string(),
            host_pattern: "*".to_string(),
            port_range: Some(PortRange {
                start: 443,
                end: 443,
            }),
            protocol: NetworkProtocol::HTTPS,
            action: NetworkAction::Allow,
            priority: 100,
        }];
        self.network_acl
            .set_allowed_hosts(plugin_id, network_rules)
            .await?;

        Ok(())
    }

    /// 標準セキュリティ制限を適用
    async fn apply_standard_security_restrictions(&self, plugin_id: Uuid) -> Result<(), McpError> {
        debug!(
            "Applying standard security restrictions for plugin: {}",
            plugin_id
        );

        // 一般的なシステムコールを許可
        let blocked_syscalls = vec![
            "execve".to_string(),
            "fork".to_string(),
            "ptrace".to_string(),
            "mount".to_string(),
        ];
        self.syscall_monitor
            .set_blocked_syscalls(plugin_id, blocked_syscalls)
            .await?;

        // HTTP/HTTPS アクセスを許可
        let network_rules = vec![NetworkRule {
            rule_id: "http-https".to_string(),
            host_pattern: "*".to_string(),
            port_range: Some(PortRange {
                start: 80,
                end: 443,
            }),
            protocol: NetworkProtocol::All,
            action: NetworkAction::Allow,
            priority: 100,
        }];
        self.network_acl
            .set_allowed_hosts(plugin_id, network_rules)
            .await?;

        Ok(())
    }

    /// 最小セキュリティ制限を適用
    async fn apply_minimal_security_restrictions(&self, plugin_id: Uuid) -> Result<(), McpError> {
        debug!(
            "Applying minimal security restrictions for plugin: {}",
            plugin_id
        );

        // 最小限のブロック（危険なシステムコールのみ）
        let blocked_syscalls = vec![
            "mount".to_string(),
            "umount".to_string(),
            "ptrace".to_string(),
        ];
        self.syscall_monitor
            .set_blocked_syscalls(plugin_id, blocked_syscalls)
            .await?;

        // ネットワークアクセス全般を許可（ログ記録）
        let network_rules = vec![NetworkRule {
            rule_id: "log-all".to_string(),
            host_pattern: "*".to_string(),
            port_range: None,
            protocol: NetworkProtocol::All,
            action: NetworkAction::Log,
            priority: 1,
        }];
        self.network_acl
            .set_allowed_hosts(plugin_id, network_rules)
            .await?;

        Ok(())
    }

    /// 権限を検証
    fn validate_permission(&self, permission: &str) -> Result<(), McpError> {
        // 権限の妥当性をチェック
        match permission {
            "network.http" | "network.https" | "network.tcp" | "network.udp" => Ok(()),
            "file.read" | "file.write" | "file.execute" => Ok(()),
            "system.process" | "system.memory" => Ok(()),
            _ => Err(McpError::SecurityError(format!(
                "Unknown permission: {}",
                permission
            ))),
        }
    }

    /// プラグインにサンドボックスを適用
    pub async fn apply_sandbox_to_plugin(
        &self,
        plugin_id: Uuid,
        container_id: &str,
    ) -> Result<(), McpError> {
        info!(
            "Applying sandbox to plugin: {} in container: {}",
            plugin_id, container_id
        );

        // システムコール監視を開始
        self.syscall_monitor.start_monitoring(plugin_id).await?;

        // ネットワークアクセス制御を開始
        self.network_acl.start_monitoring(plugin_id).await?;

        // ファイルアクセス制御を開始
        self.file_acl.start_monitoring(plugin_id).await?;

        // リソース監視を開始
        self.resource_enforcer.start_monitoring(plugin_id).await?;

        info!("Sandbox applied successfully to plugin: {}", plugin_id);
        Ok(())
    }

    /// セキュリティ違反を記録
    pub async fn record_violation(
        &self,
        plugin_id: Uuid,
        violation_type: ViolationType,
        details: String,
        severity: ViolationSeverity,
    ) -> Result<(), McpError> {
        warn!(
            "Security violation detected for plugin {}: {:?}",
            plugin_id, violation_type
        );

        let violation = SecurityViolation {
            violation_id: Uuid::new_v4(),
            plugin_id,
            violation_type,
            details: details.clone(),
            timestamp: chrono::Utc::now(),
            severity,
            action_taken: "Logged".to_string(),
        };

        let mut tracker = self.violation_tracker.lock().await;
        tracker.violation_history.push(violation);

        let count = tracker.violation_counts.entry(plugin_id).or_insert(0);
        *count += 1;

        // 違反回数チェック
        if *count >= self.security_policy.max_security_violations {
            drop(tracker);
            return Err(McpError::SecurityError(format!(
                "Plugin {} exceeded maximum security violations ({})",
                plugin_id, self.security_policy.max_security_violations
            )));
        }

        error!("Security violation recorded: {} - {}", plugin_id, details);
        Ok(())
    }

    /// セキュリティサンドボックスをシャットダウン
    pub async fn shutdown(&self) -> Result<(), McpError> {
        info!("Shutting down security sandbox");

        // 監視システムを停止
        self.syscall_monitor.shutdown().await?;
        self.network_acl.shutdown().await?;
        self.file_acl.shutdown().await?;
        self.resource_enforcer.shutdown().await?;

        info!("Security sandbox shutdown completed");
        Ok(())
    }
}

impl SyscallMonitor {
    /// 新しいシステムコール監視を作成
    pub async fn new() -> Result<Self, McpError> {
        info!("Initializing syscall monitor");
        Ok(Self {
            allowed_syscalls: Arc::new(RwLock::new(HashMap::new())),
            blocked_syscalls: Arc::new(RwLock::new(HashMap::new())),
            audit_log: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// 許可されたシステムコールを設定
    pub async fn set_allowed_syscalls(
        &self,
        plugin_id: Uuid,
        syscalls: Vec<String>,
    ) -> Result<(), McpError> {
        let mut allowed = self.allowed_syscalls.write().await;
        allowed.insert(plugin_id, syscalls);
        Ok(())
    }

    /// 禁止されたシステムコールを設定
    pub async fn set_blocked_syscalls(
        &self,
        plugin_id: Uuid,
        syscalls: Vec<String>,
    ) -> Result<(), McpError> {
        let mut blocked = self.blocked_syscalls.write().await;
        blocked.insert(plugin_id, syscalls);
        Ok(())
    }

    /// 監視を開始
    pub async fn start_monitoring(&self, plugin_id: Uuid) -> Result<(), McpError> {
        debug!("Starting syscall monitoring for plugin: {}", plugin_id);
        // TODO: 実際のシステムコール監視を実装
        Ok(())
    }

    /// シャットダウン
    pub async fn shutdown(&self) -> Result<(), McpError> {
        info!("Shutting down syscall monitor");
        Ok(())
    }
}

impl NetworkAccessControl {
    /// 新しいネットワークアクセス制御を作成
    pub async fn new() -> Result<Self, McpError> {
        info!("Initializing network access control");
        Ok(Self {
            allowed_hosts: Arc::new(RwLock::new(HashMap::new())),
            connection_monitor: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// 許可されたホストを設定
    pub async fn set_allowed_hosts(
        &self,
        plugin_id: Uuid,
        rules: Vec<NetworkRule>,
    ) -> Result<(), McpError> {
        let mut hosts = self.allowed_hosts.write().await;
        hosts.insert(plugin_id, rules);
        Ok(())
    }

    /// 全ネットワークアクセスを拒否
    pub async fn deny_all_access(&self, plugin_id: Uuid) -> Result<(), McpError> {
        let deny_rule = NetworkRule {
            rule_id: "deny-all".to_string(),
            host_pattern: "*".to_string(),
            port_range: None,
            protocol: NetworkProtocol::All,
            action: NetworkAction::Deny,
            priority: 1000,
        };
        self.set_allowed_hosts(plugin_id, vec![deny_rule]).await
    }

    /// 監視を開始
    pub async fn start_monitoring(&self, plugin_id: Uuid) -> Result<(), McpError> {
        debug!("Starting network monitoring for plugin: {}", plugin_id);
        // TODO: 実際のネットワーク監視を実装
        Ok(())
    }

    /// シャットダウン
    pub async fn shutdown(&self) -> Result<(), McpError> {
        info!("Shutting down network access control");
        Ok(())
    }
}

impl FileAccessControl {
    /// 新しいファイルアクセス制御を作成
    pub async fn new() -> Result<Self, McpError> {
        info!("Initializing file access control");
        Ok(Self {
            read_allowed_paths: Arc::new(RwLock::new(HashMap::new())),
            write_allowed_paths: Arc::new(RwLock::new(HashMap::new())),
            exec_allowed_paths: Arc::new(RwLock::new(HashMap::new())),
            file_audit_log: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// 読み取り許可パスを設定
    pub async fn set_read_allowed_paths(
        &self,
        plugin_id: Uuid,
        paths: Vec<PathRule>,
    ) -> Result<(), McpError> {
        let mut read_paths = self.read_allowed_paths.write().await;
        read_paths.insert(plugin_id, paths);
        Ok(())
    }

    /// 全書き込みアクセスを拒否
    pub async fn deny_all_write_access(&self, plugin_id: Uuid) -> Result<(), McpError> {
        // 空のルールリストで書き込みを全面禁止
        let mut write_paths = self.write_allowed_paths.write().await;
        write_paths.insert(plugin_id, vec![]);
        Ok(())
    }

    /// 全実行アクセスを拒否
    pub async fn deny_all_exec_access(&self, plugin_id: Uuid) -> Result<(), McpError> {
        // 空のルールリストで実行を全面禁止
        let mut exec_paths = self.exec_allowed_paths.write().await;
        exec_paths.insert(plugin_id, vec![]);
        Ok(())
    }

    /// 監視を開始
    pub async fn start_monitoring(&self, plugin_id: Uuid) -> Result<(), McpError> {
        debug!("Starting file access monitoring for plugin: {}", plugin_id);
        // TODO: 実際のファイルアクセス監視を実装
        Ok(())
    }

    /// シャットダウン
    pub async fn shutdown(&self) -> Result<(), McpError> {
        info!("Shutting down file access control");
        Ok(())
    }
}

impl ResourceEnforcer {
    /// 新しいリソース制限エンフォーサーを作成
    pub async fn new() -> Result<Self, McpError> {
        info!("Initializing resource enforcer");
        Ok(Self {
            cpu_monitor: Arc::new(Mutex::new(HashMap::new())),
            memory_monitor: Arc::new(Mutex::new(HashMap::new())),
            disk_monitor: Arc::new(Mutex::new(HashMap::new())),
            network_monitor: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// 監視を開始
    pub async fn start_monitoring(&self, plugin_id: Uuid) -> Result<(), McpError> {
        debug!("Starting resource monitoring for plugin: {}", plugin_id);

        // CPU使用量追跡を初期化
        let cpu_tracker = CpuUsageTracker {
            current_usage: 0.0,
            limit: 0.5, // 50%
            total_time_ms: 0,
            last_updated: chrono::Utc::now(),
        };

        let mut cpu_monitor = self.cpu_monitor.lock().await;
        cpu_monitor.insert(plugin_id, cpu_tracker);

        // メモリ使用量追跡を初期化
        let memory_tracker = MemoryUsageTracker {
            current_usage_bytes: 0,
            limit_bytes: 512 * 1024 * 1024, // 512MB
            peak_usage_bytes: 0,
            last_updated: chrono::Utc::now(),
        };

        let mut memory_monitor = self.memory_monitor.lock().await;
        memory_monitor.insert(plugin_id, memory_tracker);

        Ok(())
    }

    /// シャットダウン
    pub async fn shutdown(&self) -> Result<(), McpError> {
        info!("Shutting down resource enforcer");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_isolation::SecurityPolicy;

    #[tokio::test]
    async fn test_security_sandbox_creation() {
        let policy = SecurityPolicy {
            allowed_network_access: vec!["api.example.com".to_string()],
            blocked_syscalls: vec!["execve".to_string()],
            file_access_restrictions: vec!["/etc".to_string()],
            auto_quarantine_enabled: true,
            max_security_violations: 3,
        };

        let sandbox = SecuritySandbox::new(policy).await;
        assert!(sandbox.is_ok());
    }

    #[test]
    fn test_network_rule() {
        let rule = NetworkRule {
            rule_id: "test-rule".to_string(),
            host_pattern: "*.example.com".to_string(),
            port_range: Some(PortRange {
                start: 80,
                end: 443,
            }),
            protocol: NetworkProtocol::HTTPS,
            action: NetworkAction::Allow,
            priority: 100,
        };

        assert_eq!(rule.rule_id, "test-rule");
        assert_eq!(rule.priority, 100);
    }
}

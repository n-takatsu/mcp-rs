//! プラグイン隔離エンジン
//! 
//! 各プラグインを独立したコンテナ環境で実行し、完全な隔離を提供する

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::{RwLock, Mutex};
use tracing::{info, warn, error, debug};
use uuid::Uuid;

use crate::error::McpError;
use crate::plugin_isolation::IsolationConfig;

/// コンテナ隔離エンジン
#[derive(Debug)]
pub struct IsolationEngine {
    /// 設定
    config: IsolationConfig,
    /// アクティブなコンテナ情報
    active_containers: Arc<RwLock<HashMap<Uuid, ContainerInfo>>>,
    /// リソース使用量追跡
    resource_tracker: Arc<Mutex<ResourceTracker>>,
    /// ネットワーク隔離管理
    network_isolation: Arc<NetworkIsolation>,
    /// ファイルシステム隔離管理
    filesystem_isolation: Arc<FilesystemIsolation>,
}

/// コンテナ情報
#[derive(Debug, Clone)]
pub struct ContainerInfo {
    /// コンテナID
    pub container_id: String,
    /// プラグインID
    pub plugin_id: Uuid,
    /// 作成時刻
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// 最終アクセス時刻
    pub last_accessed: chrono::DateTime<chrono::Utc>,
    /// ネットワーク名前空間ID
    pub network_namespace: Option<String>,
    /// PIDネームスペースID
    pub pid_namespace: Option<String>,
    /// マウントポイント
    pub mount_points: Vec<MountPoint>,
    /// 環境変数
    pub environment_vars: HashMap<String, String>,
    /// リソース制限
    pub resource_limits: ContainerResourceLimits,
}

/// マウントポイント
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountPoint {
    /// ホストパス
    pub host_path: String,
    /// コンテナ内パス
    pub container_path: String,
    /// 読み取り専用フラグ
    pub readonly: bool,
    /// マウントタイプ
    pub mount_type: MountType,
}

/// マウントタイプ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MountType {
    /// バインドマウント
    Bind,
    /// ボリュームマウント
    Volume,
    /// tmpfsマウント
    Tmpfs,
}

/// コンテナリソース制限
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerResourceLimits {
    /// CPU制限（ミリコア）
    pub cpu_limit_millicores: u64,
    /// メモリ制限（バイト）
    pub memory_limit_bytes: u64,
    /// ディスクI/O制限（bytes/sec）
    pub disk_io_limit_bps: u64,
    /// ネットワーク帯域制限（bytes/sec）
    pub network_bandwidth_limit_bps: u64,
    /// ファイルディスクリプタ制限
    pub max_file_descriptors: u32,
    /// プロセス数制限
    pub max_processes: u32,
}

/// リソース使用量追跡
#[derive(Debug, Default)]
pub struct ResourceTracker {
    /// プラグイン別CPU使用率
    pub cpu_usage: HashMap<Uuid, f64>,
    /// プラグイン別メモリ使用量
    pub memory_usage: HashMap<Uuid, u64>,
    /// プラグイン別ディスク使用量
    pub disk_usage: HashMap<Uuid, u64>,
    /// プラグイン別ネットワーク使用量
    pub network_usage: HashMap<Uuid, NetworkUsage>,
}

/// ネットワーク使用量
#[derive(Debug, Clone, Default)]
pub struct NetworkUsage {
    /// 送信バイト数
    pub tx_bytes: u64,
    /// 受信バイト数
    pub rx_bytes: u64,
    /// 送信パケット数
    pub tx_packets: u64,
    /// 受信パケット数
    pub rx_packets: u64,
}

/// ネットワーク隔離管理
#[derive(Debug)]
pub struct NetworkIsolation {
    /// ネットワーク名前空間管理
    namespaces: Arc<RwLock<HashMap<Uuid, String>>>,
    /// ファイアウォールルール
    firewall_rules: Arc<RwLock<HashMap<Uuid, Vec<FirewallRule>>>>,
}

/// ファイアウォールルール
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallRule {
    /// ルールID
    pub id: String,
    /// 方向（ingress/egress）
    pub direction: TrafficDirection,
    /// プロトコル
    pub protocol: Protocol,
    /// 送信元IP/CIDR
    pub source: String,
    /// 宛先IP/CIDR
    pub destination: String,
    /// ポート範囲
    pub port_range: Option<PortRange>,
    /// アクション
    pub action: FirewallAction,
}

/// トラフィック方向
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrafficDirection {
    /// 受信
    Ingress,
    /// 送信
    Egress,
}

/// プロトコル
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Protocol {
    TCP,
    UDP,
    ICMP,
    All,
}

/// ポート範囲
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortRange {
    /// 開始ポート
    pub start: u16,
    /// 終了ポート
    pub end: u16,
}

/// ファイアウォールアクション
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FirewallAction {
    /// 許可
    Allow,
    /// 拒否
    Deny,
    /// ログ記録
    Log,
}

/// ファイルシステム隔離管理
#[derive(Debug)]
pub struct FilesystemIsolation {
    /// chroot ディレクトリ管理
    chroot_dirs: Arc<RwLock<HashMap<Uuid, String>>>,
    /// マウント名前空間管理
    mount_namespaces: Arc<RwLock<HashMap<Uuid, String>>>,
    /// 読み取り専用ファイルシステム
    readonly_filesystems: Arc<RwLock<HashMap<Uuid, Vec<String>>>>,
}

impl IsolationEngine {
    /// 新しい隔離エンジンを作成
    pub async fn new(config: IsolationConfig) -> Result<Self, McpError> {
        info!("Initializing isolation engine");

        let network_isolation = Arc::new(NetworkIsolation {
            namespaces: Arc::new(RwLock::new(HashMap::new())),
            firewall_rules: Arc::new(RwLock::new(HashMap::new())),
        });

        let filesystem_isolation = Arc::new(FilesystemIsolation {
            chroot_dirs: Arc::new(RwLock::new(HashMap::new())),
            mount_namespaces: Arc::new(RwLock::new(HashMap::new())),
            readonly_filesystems: Arc::new(RwLock::new(HashMap::new())),
        });

        // コンテナランタイムの初期化確認
        Self::verify_container_runtime(&config.container_runtime).await?;

        Ok(Self {
            config,
            active_containers: Arc::new(RwLock::new(HashMap::new())),
            resource_tracker: Arc::new(Mutex::new(ResourceTracker::default())),
            network_isolation,
            filesystem_isolation,
        })
    }

    /// コンテナランタイムの動作確認
    async fn verify_container_runtime(runtime: &str) -> Result<(), McpError> {
        debug!("Verifying container runtime: {}", runtime);

        let output = Command::new(runtime)
            .arg("--version")
            .output()
            .map_err(|e| McpError::IsolationError(
                format!("Failed to verify container runtime {}: {}", runtime, e)
            ))?;

        if !output.status.success() {
            return Err(McpError::IsolationError(
                format!("Container runtime {} is not available", runtime)
            ));
        }

        info!("Container runtime {} verified successfully", runtime);
        Ok(())
    }

    /// プラグインを隔離環境で起動
    pub async fn start_plugin(&self, plugin_id: Uuid) -> Result<String, McpError> {
        info!("Starting plugin in isolated environment: {}", plugin_id);

        // ネットワーク名前空間作成
        let network_namespace = if self.config.use_network_namespace {
            Some(self.create_network_namespace(plugin_id).await?)
        } else {
            None
        };

        // ファイルシステム隔離設定
        let mount_points = if self.config.filesystem_isolation {
            self.setup_filesystem_isolation(plugin_id).await?
        } else {
            vec![]
        };

        // コンテナ作成
        let container_id = self.create_container(
            plugin_id,
            network_namespace.clone(),
            &mount_points
        ).await?;

        // リソース制限適用
        self.apply_resource_limits(&container_id, plugin_id).await?;

        // コンテナ起動
        self.start_container(&container_id).await?;

        // コンテナ情報を記録
        let container_info = ContainerInfo {
            container_id: container_id.clone(),
            plugin_id,
            created_at: chrono::Utc::now(),
            last_accessed: chrono::Utc::now(),
            network_namespace,
            pid_namespace: None, // TODO: 実装
            mount_points,
            environment_vars: HashMap::new(),
            resource_limits: self.get_default_resource_limits(),
        };

        let mut containers = self.active_containers.write().await;
        containers.insert(plugin_id, container_info);

        info!("Plugin started in container: {} -> {}", plugin_id, container_id);
        Ok(container_id)
    }

    /// ネットワーク名前空間を作成
    async fn create_network_namespace(&self, plugin_id: Uuid) -> Result<String, McpError> {
        let namespace_name = format!("plugin-{}", plugin_id);
        
        debug!("Creating network namespace: {}", namespace_name);

        // ip netns add コマンドを実行
        let output = Command::new("ip")
            .args(&["netns", "add", &namespace_name])
            .output()
            .map_err(|e| McpError::IsolationError(
                format!("Failed to create network namespace: {}", e)
            ))?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(McpError::IsolationError(
                format!("Failed to create network namespace: {}", error_msg)
            ));
        }

        // ネットワーク名前空間を記録
        let mut namespaces = self.network_isolation.namespaces.write().await;
        namespaces.insert(plugin_id, namespace_name.clone());

        info!("Network namespace created: {}", namespace_name);
        Ok(namespace_name)
    }

    /// ファイルシステム隔離を設定
    async fn setup_filesystem_isolation(&self, plugin_id: Uuid) -> Result<Vec<MountPoint>, McpError> {
        debug!("Setting up filesystem isolation for plugin: {}", plugin_id);

        let mut mount_points = vec![];

        // 基本的な読み取り専用マウント
        mount_points.push(MountPoint {
            host_path: "/usr".to_string(),
            container_path: "/usr".to_string(),
            readonly: true,
            mount_type: MountType::Bind,
        });

        mount_points.push(MountPoint {
            host_path: "/lib".to_string(),
            container_path: "/lib".to_string(),
            readonly: true,
            mount_type: MountType::Bind,
        });

        // プラグイン専用の作業ディレクトリ
        let work_dir = format!("/tmp/plugin-{}", plugin_id);
        mount_points.push(MountPoint {
            host_path: work_dir.clone(),
            container_path: "/work".to_string(),
            readonly: false,
            mount_type: MountType::Tmpfs,
        });

        // ファイルシステム隔離情報を記録
        let mut chroot_dirs = self.filesystem_isolation.chroot_dirs.write().await;
        chroot_dirs.insert(plugin_id, work_dir);

        Ok(mount_points)
    }

    /// コンテナを作成
    async fn create_container(
        &self,
        plugin_id: Uuid,
        network_namespace: Option<String>,
        mount_points: &[MountPoint]
    ) -> Result<String, McpError> {
        let container_name = format!("plugin-{}", plugin_id);
        
        debug!("Creating container: {}", container_name);

        let mut cmd = Command::new(&self.config.container_runtime);
        cmd.args(&["create", "--name", &container_name]);

        // ネットワーク隔離設定
        if let Some(ns) = &network_namespace {
            cmd.args(&["--network", "none"]);
            cmd.args(&["--net", &format!("ns:{}", ns)]);
        }

        // マウントポイント設定
        for mount in mount_points {
            let mount_arg = if mount.readonly {
                format!("{}:{}:ro", mount.host_path, mount.container_path)
            } else {
                format!("{}:{}", mount.host_path, mount.container_path)
            };
            cmd.args(&["-v", &mount_arg]);
        }

        // セキュリティオプション
        cmd.args(&["--security-opt", "no-new-privileges"]);
        cmd.args(&["--cap-drop", "ALL"]);
        cmd.args(&["--read-only"]);

        // ベースイメージ
        cmd.arg("alpine:latest");
        
        // デフォルトコマンド
        cmd.arg("sleep");
        cmd.arg("infinity");

        let output = cmd.output()
            .map_err(|e| McpError::IsolationError(
                format!("Failed to create container: {}", e)
            ))?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(McpError::IsolationError(
                format!("Failed to create container: {}", error_msg)
            ));
        }

        // コンテナIDを取得
        let container_id = String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string();

        if container_id.is_empty() {
            return Err(McpError::IsolationError(
                "Container creation returned empty ID".to_string()
            ));
        }

        info!("Container created: {} -> {}", plugin_id, container_id);
        Ok(container_id)
    }

    /// リソース制限を適用
    async fn apply_resource_limits(&self, container_id: &str, plugin_id: Uuid) -> Result<(), McpError> {
        debug!("Applying resource limits to container: {}", container_id);

        let limits = self.get_default_resource_limits();

        // CPU制限
        let cpu_limit = format!("{}m", limits.cpu_limit_millicores);
        Command::new(&self.config.container_runtime)
            .args(&["update", "--cpus", &cpu_limit, container_id])
            .output()
            .map_err(|e| McpError::IsolationError(
                format!("Failed to set CPU limit: {}", e)
            ))?;

        // メモリ制限
        let memory_limit = format!("{}b", limits.memory_limit_bytes);
        Command::new(&self.config.container_runtime)
            .args(&["update", "--memory", &memory_limit, container_id])
            .output()
            .map_err(|e| McpError::IsolationError(
                format!("Failed to set memory limit: {}", e)
            ))?;

        info!("Resource limits applied to container: {}", container_id);
        Ok(())
    }

    /// コンテナを起動
    async fn start_container(&self, container_id: &str) -> Result<(), McpError> {
        debug!("Starting container: {}", container_id);

        let output = Command::new(&self.config.container_runtime)
            .args(&["start", container_id])
            .output()
            .map_err(|e| McpError::IsolationError(
                format!("Failed to start container: {}", e)
            ))?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(McpError::IsolationError(
                format!("Failed to start container: {}", error_msg)
            ));
        }

        info!("Container started: {}", container_id);
        Ok(())
    }

    /// プラグインを停止
    pub async fn stop_plugin(&self, plugin_id: Uuid, container_id: &str) -> Result<(), McpError> {
        info!("Stopping plugin: {} in container: {}", plugin_id, container_id);

        // コンテナを停止
        self.stop_container(container_id).await?;

        // コンテナを削除
        self.remove_container(container_id).await?;

        // ネットワーク名前空間を削除
        if self.config.use_network_namespace {
            self.cleanup_network_namespace(plugin_id).await?;
        }

        // ファイルシステム隔離をクリーンアップ
        if self.config.filesystem_isolation {
            self.cleanup_filesystem_isolation(plugin_id).await?;
        }

        // コンテナ情報を削除
        let mut containers = self.active_containers.write().await;
        containers.remove(&plugin_id);

        // リソース追跡情報を削除
        let mut tracker = self.resource_tracker.lock().await;
        tracker.cpu_usage.remove(&plugin_id);
        tracker.memory_usage.remove(&plugin_id);
        tracker.disk_usage.remove(&plugin_id);
        tracker.network_usage.remove(&plugin_id);

        info!("Plugin stopped and cleaned up: {}", plugin_id);
        Ok(())
    }

    /// プラグインを強制停止
    pub async fn force_stop_plugin(&self, plugin_id: Uuid, container_id: &str) -> Result<(), McpError> {
        warn!("Force stopping plugin: {} in container: {}", plugin_id, container_id);

        // コンテナを強制停止
        let output = Command::new(&self.config.container_runtime)
            .args(&["kill", container_id])
            .output()
            .map_err(|e| McpError::IsolationError(
                format!("Failed to force stop container: {}", e)
            ))?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            warn!("Failed to force stop container: {}", error_msg);
        }

        // 通常の停止処理を実行
        self.stop_plugin(plugin_id, container_id).await
    }

    /// コンテナを停止
    async fn stop_container(&self, container_id: &str) -> Result<(), McpError> {
        debug!("Stopping container: {}", container_id);

        let output = Command::new(&self.config.container_runtime)
            .args(&["stop", container_id])
            .output()
            .map_err(|e| McpError::IsolationError(
                format!("Failed to stop container: {}", e)
            ))?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(McpError::IsolationError(
                format!("Failed to stop container: {}", error_msg)
            ));
        }

        Ok(())
    }

    /// コンテナを削除
    async fn remove_container(&self, container_id: &str) -> Result<(), McpError> {
        debug!("Removing container: {}", container_id);

        let output = Command::new(&self.config.container_runtime)
            .args(&["rm", container_id])
            .output()
            .map_err(|e| McpError::IsolationError(
                format!("Failed to remove container: {}", e)
            ))?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(McpError::IsolationError(
                format!("Failed to remove container: {}", error_msg)
            ));
        }

        Ok(())
    }

    /// ネットワーク名前空間をクリーンアップ
    async fn cleanup_network_namespace(&self, plugin_id: Uuid) -> Result<(), McpError> {
        let mut namespaces = self.network_isolation.namespaces.write().await;
        if let Some(namespace_name) = namespaces.remove(&plugin_id) {
            debug!("Cleaning up network namespace: {}", namespace_name);

            let output = Command::new("ip")
                .args(&["netns", "delete", &namespace_name])
                .output()
                .map_err(|e| McpError::IsolationError(
                    format!("Failed to delete network namespace: {}", e)
                ))?;

            if !output.status.success() {
                let error_msg = String::from_utf8_lossy(&output.stderr);
                warn!("Failed to delete network namespace: {}", error_msg);
            }
        }

        Ok(())
    }

    /// ファイルシステム隔離をクリーンアップ
    async fn cleanup_filesystem_isolation(&self, plugin_id: Uuid) -> Result<(), McpError> {
        let mut chroot_dirs = self.filesystem_isolation.chroot_dirs.write().await;
        if let Some(chroot_dir) = chroot_dirs.remove(&plugin_id) {
            debug!("Cleaning up chroot directory: {}", chroot_dir);

            // 作業ディレクトリを削除
            if let Err(e) = std::fs::remove_dir_all(&chroot_dir) {
                warn!("Failed to remove chroot directory {}: {}", chroot_dir, e);
            }
        }

        Ok(())
    }

    /// デフォルトリソース制限を取得
    fn get_default_resource_limits(&self) -> ContainerResourceLimits {
        ContainerResourceLimits {
            cpu_limit_millicores: 500, // 0.5 CPU
            memory_limit_bytes: 512 * 1024 * 1024, // 512MB
            disk_io_limit_bps: 10 * 1024 * 1024, // 10MB/s
            network_bandwidth_limit_bps: 10 * 1024 * 1024, // 10MB/s
            max_file_descriptors: 1024,
            max_processes: 100,
        }
    }

    /// アクティブなコンテナ一覧を取得
    pub async fn get_active_containers(&self) -> HashMap<Uuid, ContainerInfo> {
        let containers = self.active_containers.read().await;
        containers.clone()
    }

    /// リソース使用量を更新
    pub async fn update_resource_usage(&self, plugin_id: Uuid) -> Result<(), McpError> {
        // TODO: 実際のリソース使用量を取得してtracker.を更新
        debug!("Updating resource usage for plugin: {}", plugin_id);
        Ok(())
    }

    /// 隔離エンジンをシャットダウン
    pub async fn shutdown(&self) -> Result<(), McpError> {
        info!("Shutting down isolation engine");

        // 全コンテナを停止
        let container_ids: Vec<(Uuid, String)> = {
            let containers = self.active_containers.read().await;
            containers.iter()
                .map(|(id, info)| (*id, info.container_id.clone()))
                .collect()
        };

        for (plugin_id, container_id) in container_ids {
            if let Err(e) = self.stop_plugin(plugin_id, &container_id).await {
                error!("Failed to stop plugin {} during shutdown: {}", plugin_id, e);
            }
        }

        info!("Isolation engine shutdown completed");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_isolation_engine_creation() {
        let config = IsolationConfig {
            container_runtime: "docker".to_string(),
            use_network_namespace: false, // テスト環境では無効
            filesystem_isolation: false,
            process_isolation: true,
        };

        // Docker が利用できない環境ではスキップ
        if Command::new("docker").arg("--version").output().is_err() {
            return;
        }

        let engine = IsolationEngine::new(config).await;
        assert!(engine.is_ok());
    }

    #[test]
    fn test_default_resource_limits() {
        let config = IsolationConfig::default();
        let engine_config = IsolationEngine {
            config,
            active_containers: Arc::new(RwLock::new(HashMap::new())),
            resource_tracker: Arc::new(Mutex::new(ResourceTracker::default())),
            network_isolation: Arc::new(NetworkIsolation {
                namespaces: Arc::new(RwLock::new(HashMap::new())),
                firewall_rules: Arc::new(RwLock::new(HashMap::new())),
            }),
            filesystem_isolation: Arc::new(FilesystemIsolation {
                chroot_dirs: Arc::new(RwLock::new(HashMap::new())),
                mount_namespaces: Arc::new(RwLock::new(HashMap::new())),
                readonly_filesystems: Arc::new(RwLock::new(HashMap::new())),
            }),
        };

        let limits = engine_config.get_default_resource_limits();
        assert!(limits.cpu_limit_millicores > 0);
        assert!(limits.memory_limit_bytes > 0);
    }
}

impl Default for IsolationConfig {
    fn default() -> Self {
        Self {
            container_runtime: "docker".to_string(),
            use_network_namespace: true,
            filesystem_isolation: true,
            process_isolation: true,
        }
    }
}
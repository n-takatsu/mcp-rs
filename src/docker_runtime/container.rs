//! Dockerコンテナ管理
//!
//! プラグイン用のコンテナのライフサイクル管理（作成、開始、停止、削除）を提供します。

use crate::docker_runtime::{DockerError, Result};
use bollard::container::{
    Config, CreateContainerOptions, ListContainersOptions, RemoveContainerOptions,
    StartContainerOptions, StopContainerOptions, WaitContainerOptions,
};
use bollard::models::{ContainerInspectResponse, HostConfig, PortBinding};
use bollard::Docker;
use futures_util::stream::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// コンテナのリソース制限
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// メモリ制限（バイト単位）
    pub memory: Option<i64>,

    /// メモリスワップ制限（バイト単位）
    pub memory_swap: Option<i64>,

    /// CPU割り当て（0.0-1.0）
    pub cpu_quota: Option<i64>,

    /// CPU期間（マイクロ秒）
    pub cpu_period: Option<i64>,

    /// CPUシェア（相対重み）
    pub cpu_shares: Option<i64>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            memory: Some(512 * 1024 * 1024),       // 512MB
            memory_swap: Some(1024 * 1024 * 1024), // 1GB
            cpu_quota: Some(50000),                // 50%
            cpu_period: Some(100000),              // 100ms
            cpu_shares: Some(1024),
        }
    }
}

/// コンテナの設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerConfig {
    /// コンテナ名
    pub name: String,

    /// 使用するイメージ
    pub image: String,

    /// 環境変数
    pub env: HashMap<String, String>,

    /// ポートマッピング（host_port -> container_port）
    pub ports: HashMap<u16, u16>,

    /// ボリュームマウント（host_path -> container_path）
    pub volumes: HashMap<String, String>,

    /// リソース制限
    pub resource_limits: ResourceLimits,

    /// ネットワークモード
    pub network_mode: Option<String>,

    /// 自動再起動ポリシー
    pub restart_policy: Option<String>,

    /// 実行コマンド
    pub command: Option<Vec<String>>,

    /// 作業ディレクトリ
    pub working_dir: Option<String>,

    /// ユーザー（セキュリティ用）
    pub user: Option<String>,
}

impl Default for ContainerConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            image: String::new(),
            env: HashMap::new(),
            ports: HashMap::new(),
            volumes: HashMap::new(),
            resource_limits: ResourceLimits::default(),
            network_mode: Some("bridge".to_string()),
            restart_policy: Some("unless-stopped".to_string()),
            command: None,
            working_dir: None,
            user: Some("nobody".to_string()), // 最小権限
        }
    }
}

/// コンテナ情報
#[derive(Debug, Clone)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub status: String,
    pub created: i64,
}

/// Dockerコンテナマネージャー
pub struct ContainerManager {
    docker: Arc<RwLock<Docker>>,
}

impl ContainerManager {
    /// 新しいContainerManagerを作成
    pub fn new(docker: Arc<RwLock<Docker>>) -> Self {
        Self { docker }
    }

    /// コンテナを作成
    pub async fn create_container(&self, config: &ContainerConfig) -> Result<String> {
        let docker = self.docker.read().await;

        tracing::info!("Creating container: {}", config.name);

        // 環境変数をVec<String>形式に変換
        let env: Vec<String> = config
            .env
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();

        // ポートバインディングを設定
        let mut port_bindings = HashMap::new();
        let mut exposed_ports = HashMap::new();

        for (host_port, container_port) in &config.ports {
            let container_port_str = format!("{}/tcp", container_port);
            exposed_ports.insert(container_port_str.clone(), HashMap::new());

            port_bindings.insert(
                container_port_str,
                Some(vec![PortBinding {
                    host_ip: Some("0.0.0.0".to_string()),
                    host_port: Some(host_port.to_string()),
                }]),
            );
        }

        // ボリュームバインドを設定
        let binds: Vec<String> = config
            .volumes
            .iter()
            .map(|(host, container)| format!("{}:{}", host, container))
            .collect();

        // ホスト設定
        let host_config = Some(HostConfig {
            binds: Some(binds),
            port_bindings: Some(port_bindings),
            memory: config.resource_limits.memory,
            memory_swap: config.resource_limits.memory_swap,
            cpu_quota: config.resource_limits.cpu_quota,
            cpu_period: config.resource_limits.cpu_period,
            cpu_shares: config.resource_limits.cpu_shares,
            network_mode: config.network_mode.clone(),
            restart_policy: config.restart_policy.as_ref().map(|_policy| {
                bollard::models::RestartPolicy {
                    name: Some(bollard::models::RestartPolicyNameEnum::UNLESS_STOPPED),
                    maximum_retry_count: None,
                }
            }),
            ..Default::default()
        });

        // コンテナ設定
        let container_config = Config {
            image: Some(config.image.clone()),
            env: Some(env),
            exposed_ports: Some(exposed_ports),
            host_config,
            cmd: config.command.clone(),
            working_dir: config.working_dir.clone(),
            user: config.user.clone(),
            ..Default::default()
        };

        let options = CreateContainerOptions {
            name: config.name.clone(),
            platform: None,
        };

        let response = docker
            .create_container(Some(options), container_config)
            .await
            .map_err(|e| {
                DockerError::CreationFailed(format!(
                    "Failed to create container {}: {}",
                    config.name, e
                ))
            })?;

        tracing::info!(
            "Successfully created container: {} (ID: {})",
            config.name,
            response.id
        );
        Ok(response.id)
    }

    /// コンテナを開始
    pub async fn start_container(&self, container_id: &str) -> Result<()> {
        let docker = self.docker.read().await;

        tracing::info!("Starting container: {}", container_id);

        docker
            .start_container(container_id, None::<StartContainerOptions<String>>)
            .await
            .map_err(|e| {
                DockerError::StartFailed(format!(
                    "Failed to start container {}: {}",
                    container_id, e
                ))
            })?;

        tracing::info!("Successfully started container: {}", container_id);
        Ok(())
    }

    /// コンテナを停止
    pub async fn stop_container(&self, container_id: &str, timeout: Option<i64>) -> Result<()> {
        let docker = self.docker.read().await;

        tracing::info!("Stopping container: {}", container_id);

        let options = StopContainerOptions {
            t: timeout.unwrap_or(10),
        };

        docker
            .stop_container(container_id, Some(options))
            .await
            .map_err(|e| {
                DockerError::StopFailed(format!("Failed to stop container {}: {}", container_id, e))
            })?;

        tracing::info!("Successfully stopped container: {}", container_id);
        Ok(())
    }

    /// コンテナを再起動
    pub async fn restart_container(&self, container_id: &str) -> Result<()> {
        tracing::info!("Restarting container: {}", container_id);

        self.stop_container(container_id, Some(5)).await?;
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        self.start_container(container_id).await?;

        Ok(())
    }

    /// コンテナを削除
    pub async fn remove_container(&self, container_id: &str, force: bool) -> Result<()> {
        let docker = self.docker.read().await;

        tracing::info!("Removing container: {}", container_id);

        let options = Some(RemoveContainerOptions {
            force,
            v: true, // ボリュームも削除
            ..Default::default()
        });

        docker
            .remove_container(container_id, options)
            .await
            .map_err(|e| {
                DockerError::ApiError(format!(
                    "Failed to remove container {}: {}",
                    container_id, e
                ))
            })?;

        tracing::info!("Successfully removed container: {}", container_id);
        Ok(())
    }

    /// すべてのコンテナをリスト
    pub async fn list_containers(&self, all: bool) -> Result<Vec<ContainerInfo>> {
        let docker = self.docker.read().await;

        let options = Some(ListContainersOptions::<String> {
            all,
            ..Default::default()
        });

        let containers = docker
            .list_containers(options)
            .await
            .map_err(|e| DockerError::ApiError(format!("Failed to list containers: {}", e)))?;

        Ok(containers
            .into_iter()
            .map(|c| ContainerInfo {
                id: c.id.unwrap_or_default(),
                name: c
                    .names
                    .and_then(|n| n.into_iter().next())
                    .unwrap_or_default(),
                image: c.image.unwrap_or_default(),
                state: c.state.unwrap_or_default(),
                status: c.status.unwrap_or_default(),
                created: c.created.unwrap_or_default(),
            })
            .collect())
    }

    /// コンテナが実行中かチェック
    pub async fn is_running(&self, container_id: &str) -> Result<bool> {
        let info = self.inspect_container(container_id).await?;

        Ok(info.state.and_then(|s| s.running).unwrap_or(false))
    }

    /// コンテナ情報を取得
    pub async fn inspect_container(&self, container_id: &str) -> Result<ContainerInspectResponse> {
        let docker = self.docker.read().await;

        docker
            .inspect_container(container_id, None)
            .await
            .map_err(|e| {
                DockerError::ContainerNotFound(format!(
                    "Container {} not found: {}",
                    container_id, e
                ))
            })
    }

    /// コンテナのログを取得
    pub async fn get_logs(&self, container_id: &str, tail: Option<usize>) -> Result<Vec<String>> {
        let docker = self.docker.read().await;

        use bollard::container::LogsOptions;

        let options = Some(LogsOptions::<String> {
            stdout: true,
            stderr: true,
            tail: tail
                .map(|t| t.to_string())
                .unwrap_or_else(|| "100".to_string()),
            ..Default::default()
        });

        let mut stream = docker.logs(container_id, options);
        let mut logs = Vec::new();

        while let Some(result) = stream.next().await {
            match result {
                Ok(log_output) => {
                    logs.push(log_output.to_string());
                }
                Err(e) => {
                    tracing::warn!("Error reading logs: {}", e);
                    break;
                }
            }
        }

        Ok(logs)
    }

    /// コンテナの実行を待機
    pub async fn wait_container(&self, container_id: &str) -> Result<i64> {
        let docker = self.docker.read().await;

        let options = Some(WaitContainerOptions {
            condition: "not-running",
        });

        let mut stream = docker.wait_container(container_id, options);

        if let Some(result) = stream.next().await {
            let response = result.map_err(|e| {
                DockerError::ApiError(format!("Error waiting for container: {}", e))
            })?;

            Ok(response.status_code)
        } else {
            Err(DockerError::ApiError("No response from wait".to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bollard::Docker;

    async fn setup() -> ContainerManager {
        let docker = Docker::connect_with_socket_defaults().unwrap();
        ContainerManager::new(Arc::new(RwLock::new(docker)))
    }

    #[tokio::test]
    #[ignore] // Docker環境が必要
    async fn test_list_containers() {
        let manager = setup().await;
        let containers = manager.list_containers(true).await;
        assert!(containers.is_ok());
    }

    #[tokio::test]
    #[ignore] // Docker環境とイメージが必要
    async fn test_create_start_stop_remove_container() {
        let manager = setup().await;

        let config = ContainerConfig {
            name: "test-container".to_string(),
            image: "alpine:latest".to_string(),
            command: Some(vec!["sleep".to_string(), "10".to_string()]),
            ..Default::default()
        };

        // 作成
        let container_id = manager.create_container(&config).await.unwrap();

        // 開始
        manager.start_container(&container_id).await.unwrap();

        // 実行中確認
        let running = manager.is_running(&container_id).await.unwrap();
        assert!(running);

        // 停止
        manager
            .stop_container(&container_id, Some(5))
            .await
            .unwrap();

        // 削除
        manager.remove_container(&container_id, true).await.unwrap();
    }
}

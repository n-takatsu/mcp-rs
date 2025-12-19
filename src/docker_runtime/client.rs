//! Docker API クライアント
//!
//! DockerデーモンとのHTTP通信を管理します。

use bollard::Docker;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::docker_runtime::{DockerError, Result};

/// Docker APIクライアント
pub struct DockerClient {
    /// Bollard Dockerクライアント
    docker: Arc<RwLock<Docker>>,
}

impl DockerClient {
    /// 新しいDockerクライアントを作成（Unix socket接続）
    pub async fn new() -> Result<Self> {
        let docker = Docker::connect_with_socket_defaults()
            .map_err(|e| DockerError::ApiError(format!("Failed to connect to Docker: {}", e)))?;
        
        Ok(Self {
            docker: Arc::new(RwLock::new(docker)),
        })
    }

    /// 新しいDockerクライアントを作成（HTTP接続）
    pub async fn new_with_http(url: &str) -> Result<Self> {
        let docker = Docker::connect_with_http(url, 120, bollard::API_DEFAULT_VERSION)
            .map_err(|e| DockerError::ApiError(format!("Failed to connect to Docker: {}", e)))?;
        
        Ok(Self {
            docker: Arc::new(RwLock::new(docker)),
        })
    }

    /// Dockerデーモンのバージョン情報を取得
    pub async fn version(&self) -> Result<bollard::models::Version> {
        let docker = self.docker.read().await;
        docker.version()
            .await
            .map_err(|e| DockerError::ApiError(format!("Failed to get Docker version: {}", e)))
    }

    /// Dockerデーモンの情報を取得
    pub async fn info(&self) -> Result<bollard::models::SystemInfo> {
        let docker = self.docker.read().await;
        docker.info()
            .await
            .map_err(|e| DockerError::ApiError(format!("Failed to get Docker info: {}", e)))
    }

    /// Dockerデーモンが利用可能かをチェック
    pub async fn ping(&self) -> Result<bool> {
        let docker = self.docker.read().await;
        match docker.ping().await {
            Ok(_) => Ok(true),
            Err(e) => {
                tracing::warn!("Docker ping failed: {}", e);
                Ok(false)
            }
        }
    }

    /// 内部のDockerクライアントへの参照を取得
    pub(crate) fn inner(&self) -> Arc<RwLock<Docker>> {
        Arc::clone(&self.docker)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Docker環境が必要
    async fn test_docker_client_connection() {
        let client = DockerClient::new().await;
        assert!(client.is_ok(), "Failed to create Docker client");
    }

    #[tokio::test]
    #[ignore] // Docker環境が必要
    async fn test_docker_ping() {
        let client = DockerClient::new().await.unwrap();
        let result = client.ping().await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    #[ignore] // Docker環境が必要
    async fn test_docker_version() {
        let client = DockerClient::new().await.unwrap();
        let version = client.version().await;
        assert!(version.is_ok());
        println!("Docker version: {:?}", version.unwrap());
    }

    #[tokio::test]
    #[ignore] // Docker環境が必要
    async fn test_docker_info() {
        let client = DockerClient::new().await.unwrap();
        let info = client.info().await;
        assert!(info.is_ok());
        println!("Docker info: {:?}", info.unwrap());
    }
}

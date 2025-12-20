//! Dockerイメージ管理
//!
//! プラグイン用のDockerイメージの管理（pull, build, list, remove）を提供します。

use crate::docker_runtime::{DockerError, Result};
use bollard::image::{
    BuildImageOptions, CreateImageOptions, ListImagesOptions, RemoveImageOptions,
};
use bollard::Docker;
use futures_util::stream::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Dockerイメージの設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageConfig {
    /// イメージ名（例: "rust:1.70-alpine"）
    pub name: String,

    /// イメージタグ（デフォルト: "latest"）
    pub tag: String,

    /// レジストリURL（オプション）
    pub registry: Option<String>,

    /// 認証情報（オプション）
    pub auth: Option<ImageAuth>,
}

/// イメージの認証情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageAuth {
    pub username: String,
    pub password: String,
    pub server_address: Option<String>,
}

/// イメージ情報
#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub id: String,
    pub repo_tags: Vec<String>,
    pub size: i64,
    pub created: i64,
}

/// Dockerイメージマネージャー
pub struct ImageManager {
    docker: Arc<RwLock<Docker>>,
}

impl ImageManager {
    /// 新しいImageManagerを作成
    pub fn new(docker: Arc<RwLock<Docker>>) -> Self {
        Self { docker }
    }

    /// イメージをpull（ダウンロード）
    pub async fn pull_image(&self, config: &ImageConfig) -> Result<()> {
        let docker = self.docker.read().await;

        let image_name = if let Some(registry) = &config.registry {
            format!("{}/{}:{}", registry, config.name, config.tag)
        } else {
            format!("{}:{}", config.name, config.tag)
        };

        tracing::info!("Pulling Docker image: {}", image_name);

        let options = Some(CreateImageOptions {
            from_image: image_name.clone(),
            ..Default::default()
        });

        let mut stream = docker.create_image(options, None, None);

        while let Some(result) = stream.next().await {
            match result {
                Ok(info) => {
                    if let Some(status) = info.status {
                        tracing::debug!("Pull status: {}", status);
                    }
                    if let Some(error) = info.error {
                        return Err(DockerError::ApiError(format!(
                            "Failed to pull image {}: {}",
                            image_name, error
                        )));
                    }
                }
                Err(e) => {
                    return Err(DockerError::ApiError(format!(
                        "Error during image pull: {}",
                        e
                    )));
                }
            }
        }

        tracing::info!("Successfully pulled image: {}", image_name);
        Ok(())
    }

    /// イメージを構築
    pub async fn build_image(
        &self,
        dockerfile_path: &str,
        tag: &str,
        build_args: Option<HashMap<String, String>>,
    ) -> Result<String> {
        let docker = self.docker.read().await;

        tracing::info!("Building Docker image with tag: {}", tag);

        let options = BuildImageOptions {
            dockerfile: "Dockerfile".to_string(),
            t: tag.to_string(),
            rm: true,
            buildargs: build_args.unwrap_or_default(),
            ..Default::default()
        };

        // Dockerfileの内容を読み込む
        let dockerfile_content = tokio::fs::read(dockerfile_path).await?;

        let mut stream = docker.build_image(options, None, Some(dockerfile_content.into()));

        let mut image_id = String::new();

        while let Some(result) = stream.next().await {
            match result {
                Ok(info) => {
                    if let Some(stream) = info.stream {
                        tracing::debug!("Build output: {}", stream.trim());
                    }
                    if let Some(error) = info.error {
                        return Err(DockerError::ApiError(format!("Build failed: {}", error)));
                    }
                    // Extract image ID from aux field
                    if let Some(aux) = info.aux {
                        if let Some(id_str) = aux.id {
                            image_id = id_str;
                        }
                    }
                }
                Err(e) => {
                    return Err(DockerError::ApiError(format!(
                        "Error during image build: {}",
                        e
                    )));
                }
            }
        }

        tracing::info!("Successfully built image: {} (ID: {})", tag, image_id);
        Ok(image_id)
    }

    /// すべてのイメージをリスト
    pub async fn list_images(&self) -> Result<Vec<ImageInfo>> {
        let docker = self.docker.read().await;

        let options = Some(ListImagesOptions::<String> {
            all: false,
            ..Default::default()
        });

        let images = docker
            .list_images(options)
            .await
            .map_err(|e| DockerError::ApiError(format!("Failed to list images: {}", e)))?;

        Ok(images
            .into_iter()
            .map(|img| ImageInfo {
                id: img.id,
                repo_tags: img.repo_tags,
                size: img.size,
                created: img.created,
            })
            .collect())
    }

    /// イメージを削除
    pub async fn remove_image(&self, image_name: &str, force: bool) -> Result<()> {
        let docker = self.docker.read().await;

        tracing::info!("Removing Docker image: {}", image_name);

        let options = Some(RemoveImageOptions {
            force,
            noprune: false,
        });

        docker
            .remove_image(image_name, options, None)
            .await
            .map_err(|e| {
                DockerError::ApiError(format!("Failed to remove image {}: {}", image_name, e))
            })?;

        tracing::info!("Successfully removed image: {}", image_name);
        Ok(())
    }

    /// イメージが存在するかチェック
    pub async fn image_exists(&self, image_name: &str) -> Result<bool> {
        let images = self.list_images().await?;

        Ok(images
            .iter()
            .any(|img| img.repo_tags.iter().any(|tag| tag.contains(image_name))))
    }

    /// イメージ情報を取得
    pub async fn inspect_image(&self, image_name: &str) -> Result<bollard::models::ImageInspect> {
        let docker = self.docker.read().await;

        docker.inspect_image(image_name).await.map_err(|e| {
            DockerError::ImageNotFound(format!("Image {} not found: {}", image_name, e))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bollard::Docker;

    async fn setup() -> ImageManager {
        let docker = Docker::connect_with_socket_defaults().unwrap();
        ImageManager::new(Arc::new(RwLock::new(docker)))
    }

    #[tokio::test]
    #[ignore] // Docker環境が必要
    async fn test_list_images() {
        let manager = setup().await;
        let images = manager.list_images().await;
        assert!(images.is_ok());
    }

    #[tokio::test]
    #[ignore] // Docker環境が必要
    async fn test_image_exists() {
        let manager = setup().await;
        // 一般的に存在しないイメージでテスト
        let exists = manager.image_exists("nonexistent-image-12345").await;
        assert!(exists.is_ok());
        assert!(!exists.unwrap());
    }
}

//! Docker Runtime Integration Module
//!
//! このモジュールはプラグインのDocker統合機能を提供します。
//! プラグインを独立したDockerコンテナで実行し、強力な隔離とセキュリティを実現します。

mod client;
mod image;
mod container;
mod monitoring;
mod security;

pub use client::DockerClient;
pub use image::{ImageManager, ImageConfig, ImageInfo};
pub use container::{ContainerManager, ContainerConfig, ContainerInfo, ResourceLimits};
pub use monitoring::{MonitoringManager, ContainerMetrics, HealthStatus};
pub use security::{SecurityManager, SecurityProfile, SecretManager};

use thiserror::Error;

/// Docker統合のエラー型
#[derive(Error, Debug)]
pub enum DockerError {
    #[error("Docker API error: {0}")]
    ApiError(String),
    
    #[error("Container not found: {0}")]
    ContainerNotFound(String),
    
    #[error("Image not found: {0}")]
    ImageNotFound(String),
    
    #[error("Container creation failed: {0}")]
    CreationFailed(String),
    
    #[error("Container start failed: {0}")]
    StartFailed(String),
    
    #[error("Container stop failed: {0}")]
    StopFailed(String),
    
    #[error("Network error: {0}")]
    NetworkError(String),
    
    #[error("Security violation: {0}")]
    SecurityViolation(String),
    
    #[error("Resource limit exceeded: {0}")]
    ResourceLimitExceeded(String),
    
    #[error("Health check failed: {0}")]
    HealthCheckFailed(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, DockerError>;

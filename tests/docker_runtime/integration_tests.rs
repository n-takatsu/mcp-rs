//! Docker Runtime統合の統合テスト

use mcp_rs::docker_runtime::*;

#[cfg(test)]
mod integration_tests {
    use super::*;

    /// Docker環境のセットアップ確認
    #[tokio::test]
    #[ignore] // Docker環境が必要
    async fn test_docker_environment_setup() {
        let client = DockerClient::new().await;
        assert!(client.is_ok(), "Failed to connect to Docker daemon");
        
        let client = client.unwrap();
        let ping_result = client.ping().await;
        assert!(ping_result.is_ok());
        assert!(ping_result.unwrap(), "Docker daemon is not responding");
    }

    /// イメージのpullとリスト
    #[tokio::test]
    #[ignore] // Docker環境とネットワークが必要
    async fn test_image_pull_and_list() {
        let client = DockerClient::new().await.unwrap();
        let manager = ImageManager::new(client.inner());

        // Alpine Linuxイメージをpull
        let config = ImageConfig {
            name: "alpine".to_string(),
            tag: "latest".to_string(),
            registry: None,
            auth: None,
        };

        let pull_result = manager.pull_image(&config).await;
        assert!(pull_result.is_ok(), "Failed to pull image");

        // イメージリスト確認
        let images = manager.list_images().await.unwrap();
        assert!(!images.is_empty(), "No images found");

        // イメージ存在確認
        let exists = manager.image_exists("alpine").await.unwrap();
        assert!(exists, "Alpine image should exist");
    }

    /// コンテナのライフサイクル管理
    #[tokio::test]
    #[ignore] // Docker環境が必要
    async fn test_container_lifecycle() {
        let client = DockerClient::new().await.unwrap();
        let manager = ContainerManager::new(client.inner());

        // コンテナ設定
        let mut config = ContainerConfig::default();
        config.name = format!("test-container-{}", uuid::Uuid::new_v4());
        config.image = "alpine:latest".to_string();
        config.command = Some(vec!["sleep".to_string(), "30".to_string()]);

        // コンテナ作成
        let container_id = manager.create_container(&config).await;
        assert!(container_id.is_ok(), "Failed to create container");
        let container_id = container_id.unwrap();

        // コンテナ開始
        let start_result = manager.start_container(&container_id).await;
        assert!(start_result.is_ok(), "Failed to start container");

        // 実行中確認
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        let is_running = manager.is_running(&container_id).await.unwrap();
        assert!(is_running, "Container should be running");

        // コンテナ情報取得
        let inspect = manager.inspect_container(&container_id).await;
        assert!(inspect.is_ok(), "Failed to inspect container");

        // コンテナ停止
        let stop_result = manager.stop_container(&container_id, Some(5)).await;
        assert!(stop_result.is_ok(), "Failed to stop container");

        // コンテナ削除
        let remove_result = manager.remove_container(&container_id, true).await;
        assert!(remove_result.is_ok(), "Failed to remove container");
    }

    /// リソース制限のテスト
    #[tokio::test]
    #[ignore] // Docker環境が必要
    async fn test_resource_limits() {
        let client = DockerClient::new().await.unwrap();
        let manager = ContainerManager::new(client.inner());

        // リソース制限付きコンテナ設定
        let mut config = ContainerConfig::default();
        config.name = format!("test-limited-{}", uuid::Uuid::new_v4());
        config.image = "alpine:latest".to_string();
        config.command = Some(vec!["sleep".to_string(), "10".to_string()]);
        config.resource_limits = ResourceLimits {
            memory: Some(128 * 1024 * 1024), // 128MB
            memory_swap: Some(256 * 1024 * 1024), // 256MB
            cpu_quota: Some(25000), // 25%
            cpu_period: Some(100000),
            cpu_shares: Some(512),
        };

        let container_id = manager.create_container(&config).await.unwrap();
        manager.start_container(&container_id).await.unwrap();

        // コンテナ情報確認
        let inspect = manager.inspect_container(&container_id).await.unwrap();
        let host_config = inspect.host_config.unwrap();
        
        assert_eq!(host_config.memory, Some(128 * 1024 * 1024));
        assert_eq!(host_config.cpu_quota, Some(25000));

        // クリーンアップ
        manager.stop_container(&container_id, Some(5)).await.ok();
        manager.remove_container(&container_id, true).await.ok();
    }

    /// モニタリング機能のテスト
    #[tokio::test]
    #[ignore] // Docker環境が必要
    async fn test_monitoring() {
        let client = DockerClient::new().await.unwrap();
        let container_manager = ContainerManager::new(client.inner());
        let monitoring_manager = MonitoringManager::new(client.inner());

        // テスト用コンテナ起動
        let mut config = ContainerConfig::default();
        config.name = format!("test-monitoring-{}", uuid::Uuid::new_v4());
        config.image = "alpine:latest".to_string();
        config.command = Some(vec!["sleep".to_string(), "20".to_string()]);

        let container_id = container_manager.create_container(&config).await.unwrap();
        container_manager.start_container(&container_id).await.unwrap();

        // コンテナが起動するまで待機
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // メトリクス収集
        let metrics = monitoring_manager.collect_metrics(&container_id).await;
        assert!(metrics.is_ok(), "Failed to collect metrics");
        
        let metrics = metrics.unwrap();
        assert_eq!(metrics.container_id, container_id);
        assert!(metrics.cpu_usage_percent >= 0.0);
        assert!(metrics.memory_usage_bytes > 0);

        // ヘルスチェック
        let health = monitoring_manager.check_health(&container_id).await;
        assert!(health.is_ok(), "Failed to check health");

        // クリーンアップ
        container_manager.stop_container(&container_id, Some(5)).await.ok();
        container_manager.remove_container(&container_id, true).await.ok();
    }

    /// セキュリティプロファイルのテスト
    #[tokio::test]
    async fn test_security_profiles() {
        // 最小権限プロファイル
        let minimal = SecurityProfile::minimal();
        assert_eq!(minimal.level, SecurityLevel::Minimal);
        assert!(minimal.read_only_rootfs);
        assert!(!minimal.privileged);
        assert!(minimal.no_new_privileges);
        assert_eq!(minimal.cap_drop, vec!["ALL"]);

        // 標準プロファイル
        let standard = SecurityProfile::standard();
        assert_eq!(standard.level, SecurityLevel::Standard);
        assert!(!standard.privileged);
        assert!(standard.no_new_privileges);
    }

    /// シークレット管理のテスト
    #[tokio::test]
    async fn test_secret_management() {
        let encryption_key = b"test-encryption-key-for-testing!".to_vec();
        let secret_manager = SecretManager::new(encryption_key);

        // シークレット追加
        let add_result = secret_manager.add_secret(
            "api_key".to_string(),
            "secret-api-key-12345".to_string()
        ).await;
        assert!(add_result.is_ok(), "Failed to add secret");

        // シークレット取得
        let value = secret_manager.get_secret("api_key").await;
        assert!(value.is_ok(), "Failed to get secret");
        assert_eq!(value.unwrap(), "secret-api-key-12345");

        // シークレットリスト
        let secrets = secret_manager.list_secrets().await;
        assert!(secrets.contains(&"api_key".to_string()));

        // シークレット削除
        let remove_result = secret_manager.remove_secret("api_key").await;
        assert!(remove_result.is_ok(), "Failed to remove secret");

        // 削除後は取得できない
        let not_found = secret_manager.get_secret("api_key").await;
        assert!(not_found.is_err());
    }

    /// セキュリティマネージャーのテスト
    #[tokio::test]
    async fn test_security_manager() {
        let encryption_key = b"test-encryption-key-for-testing!".to_vec();
        let mut manager = SecurityManager::new(encryption_key);

        // デフォルトプロファイル確認
        let default = manager.get_default_profile();
        assert_eq!(default.level, SecurityLevel::Standard);

        // カスタムプロファイル追加
        let custom_profile = SecurityProfile::minimal();
        manager.add_profile("minimal".to_string(), custom_profile).await;

        // カスタムプロファイル取得
        let retrieved = manager.get_profile("minimal").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().level, SecurityLevel::Minimal);

        // セキュリティ検証
        let safe_profile = SecurityProfile::standard();
        assert!(manager.validate_config(&safe_profile).is_ok());

        let mut dangerous_profile = SecurityProfile::standard();
        dangerous_profile.privileged = true;
        assert!(manager.validate_config(&dangerous_profile).is_err());
    }

    /// ネットワーク分離のテスト
    #[tokio::test]
    #[ignore] // Docker環境が必要
    async fn test_network_isolation() {
        let client = DockerClient::new().await.unwrap();
        let manager = ContainerManager::new(client.inner());

        // 独立したネットワークでコンテナを起動
        let mut config = ContainerConfig::default();
        config.name = format!("test-network-{}", uuid::Uuid::new_v4());
        config.image = "alpine:latest".to_string();
        config.command = Some(vec!["sleep".to_string(), "10".to_string()]);
        config.network_mode = Some("none".to_string()); // ネットワークなし

        let container_id = manager.create_container(&config).await;
        assert!(container_id.is_ok(), "Failed to create isolated container");

        let container_id = container_id.unwrap();
        manager.start_container(&container_id).await.unwrap();

        // ネットワーク設定確認
        let inspect = manager.inspect_container(&container_id).await.unwrap();
        let network_mode = inspect.host_config
            .and_then(|hc| hc.network_mode)
            .unwrap_or_default();
        assert_eq!(network_mode, "none");

        // クリーンアップ
        manager.stop_container(&container_id, Some(5)).await.ok();
        manager.remove_container(&container_id, true).await.ok();
    }
}

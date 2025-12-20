# Docker Runtime統合ガイド

## 概要

このガイドでは、MCP-RSのDocker Runtime統合機能の使用方法について説明します。Docker統合により、プラグインを独立したDockerコンテナで実行し、強力な隔離とセキュリティを実現します。

## 目次

1. [前提条件](#前提条件)
2. [基本的な使い方](#基本的な使い方)
3. [イメージ管理](#イメージ管理)
4. [コンテナ管理](#コンテナ管理)
5. [監視とログ](#監視とログ)
6. [セキュリティ](#セキュリティ)
7. [トラブルシューティング](#トラブルシューティング)

## 前提条件

### Docker環境

```bash
# Dockerがインストールされていることを確認
docker --version

# Dockerデーモンが起動していることを確認
docker ps
```

### Cargo.tomlの設定

```toml
[dependencies]
mcp-rs = { version = "0.15.1", features = ["docker-runtime"] }
```

## 基本的な使い方

### Dockerクライアントの初期化

```rust
use mcp_rs::docker_runtime::DockerClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Unix socket経由で接続（Linux/Mac）
    let client = DockerClient::new().await?;
    
    // または HTTP経由で接続
    // let client = DockerClient::new_with_http("http://localhost:2375").await?;
    
    // 接続確認
    if client.ping().await? {
        println!("Docker daemon is running");
    }
    
    Ok(())
}
```

## イメージ管理

### イメージのPull

```rust
use mcp_rs::docker_runtime::{ImageManager, ImageConfig};

let manager = ImageManager::new(client.inner());

let config = ImageConfig {
    name: "rust".to_string(),
    tag: "1.70-alpine".to_string(),
    registry: None,
    auth: None,
};

manager.pull_image(&config).await?;
```

### イメージのビルド

```rust
use std::collections::HashMap;

let mut build_args = HashMap::new();
build_args.insert("VERSION".to_string(), "1.0.0".to_string());

let image_id = manager.build_image(
    "./Dockerfile",
    "my-plugin:latest",
    Some(build_args)
).await?;

println!("Built image: {}", image_id);
```

### イメージのリスト

```rust
let images = manager.list_images().await?;

for image in images {
    println!("Image: {:?}", image.repo_tags);
    println!("Size: {} MB", image.size / 1024 / 1024);
}
```

## コンテナ管理

### コンテナの作成と起動

```rust
use mcp_rs::docker_runtime::{ContainerManager, ContainerConfig, ResourceLimits};
use std::collections::HashMap;

let manager = ContainerManager::new(client.inner());

// 環境変数の設定
let mut env = HashMap::new();
env.insert("PLUGIN_NAME".to_string(), "my-plugin".to_string());
env.insert("LOG_LEVEL".to_string(), "debug".to_string());

// ポートマッピング（host_port -> container_port）
let mut ports = HashMap::new();
ports.insert(8080, 3000);

// ボリュームマウント
let mut volumes = HashMap::new();
volumes.insert("/host/data".to_string(), "/container/data".to_string());

// リソース制限
let resource_limits = ResourceLimits {
    memory: Some(512 * 1024 * 1024), // 512MB
    memory_swap: Some(1024 * 1024 * 1024), // 1GB
    cpu_quota: Some(50000), // 50%
    cpu_period: Some(100000),
    cpu_shares: Some(1024),
};

// コンテナ設定
let config = ContainerConfig {
    name: "my-plugin-container".to_string(),
    image: "my-plugin:latest".to_string(),
    env,
    ports,
    volumes,
    resource_limits,
    network_mode: Some("bridge".to_string()),
    restart_policy: Some("unless-stopped".to_string()),
    command: Some(vec!["./plugin".to_string(), "--mode".to_string(), "production".to_string()]),
    working_dir: Some("/app".to_string()),
    user: Some("nobody".to_string()), // セキュリティのため
};

// コンテナ作成
let container_id = manager.create_container(&config).await?;

// コンテナ起動
manager.start_container(&container_id).await?;
```

### コンテナの停止と削除

```rust
// コンテナ停止（10秒のタイムアウト）
manager.stop_container(&container_id, Some(10)).await?;

// コンテナ削除（強制削除）
manager.remove_container(&container_id, true).await?;
```

### コンテナの再起動

```rust
manager.restart_container(&container_id).await?;
```

### コンテナ情報の取得

```rust
// 実行中確認
if manager.is_running(&container_id).await? {
    println!("Container is running");
}

// 詳細情報
let inspect = manager.inspect_container(&container_id).await?;
println!("Container state: {:?}", inspect.state);

// ログ取得（最新100行）
let logs = manager.get_logs(&container_id, Some(100)).await?;
for log in logs {
    println!("{}", log);
}
```

## 監視とログ

### メトリクス収集

```rust
use mcp_rs::docker_runtime::MonitoringManager;

let monitoring = MonitoringManager::new(client.inner());

// メトリクス収集
let metrics = monitoring.collect_metrics(&container_id).await?;

println!("CPU Usage: {:.2}%", metrics.cpu_usage_percent);
println!("Memory Usage: {} MB / {} MB ({:.2}%)",
    metrics.memory_usage_bytes / 1024 / 1024,
    metrics.memory_limit_bytes / 1024 / 1024,
    metrics.memory_usage_percent
);
println!("Network RX: {} bytes", metrics.network_rx_bytes);
println!("Network TX: {} bytes", metrics.network_tx_bytes);
```

### ヘルスチェック

```rust
use mcp_rs::docker_runtime::HealthStatus;

let health = monitoring.check_health(&container_id).await?;

match health {
    HealthStatus::Healthy => println!("Container is healthy"),
    HealthStatus::Unhealthy => println!("Container is unhealthy"),
    HealthStatus::Starting => println!("Container is starting"),
    HealthStatus::Unknown => println!("Health status unknown"),
}
```

### リソース閾値チェック

```rust
// CPU 80%, メモリ 90% の閾値でチェック
if monitoring.check_resource_limits(&container_id, 80.0, 90.0).await? {
    println!("Resource limits exceeded!");
}
```

### 継続的な監視

```rust
// 30秒ごとにメトリクスを収集
let container_ids = vec![container_id.clone()];
monitoring.start_monitoring(container_ids, 30).await;

// バックグラウンドで監視が継続される
```

## セキュリティ

### セキュリティプロファイル

```rust
use mcp_rs::docker_runtime::{SecurityProfile, SecurityLevel};

// 最小権限プロファイル（最も制限的）
let minimal_profile = SecurityProfile::minimal();

// 標準プロファイル
let standard_profile = SecurityProfile::standard();

// カスタムプロファイル
let custom_profile = SecurityProfile {
    level: SecurityLevel::Custom,
    read_only_rootfs: true,
    privileged: false,
    no_new_privileges: true,
    cap_add: vec!["NET_BIND_SERVICE".to_string()], // ポート80/443のバインド許可
    cap_drop: vec!["ALL".to_string()],
    apparmor_profile: Some("docker-default".to_string()),
    seccomp_profile: Some("runtime/default".to_string()),
    selinux_label: None,
    user: Some("1000:1000".to_string()),
    devices: vec![],
};
```

### セキュリティマネージャー

```rust
use mcp_rs::docker_runtime::SecurityManager;

// 暗号化キー（本番環境では安全に管理）
let encryption_key = b"your-32-byte-encryption-key!!!!!".to_vec();
let mut security = SecurityManager::new(encryption_key);

// デフォルトプロファイル設定
security.set_default_profile(SecurityProfile::minimal());

// カスタムプロファイル追加
security.add_profile("high-security".to_string(), minimal_profile).await;

// プロファイル取得
let profile = security.get_profile("high-security").await.unwrap();

// セキュリティ検証
if let Err(e) = security.validate_config(&profile) {
    eprintln!("Security violation: {}", e);
}
```

### シークレット管理

```rust
let secrets = security.secrets();

// シークレット追加
secrets.add_secret("api_key".to_string(), "secret-value-12345".to_string()).await?;

// シークレット取得
let api_key = secrets.get_secret("api_key").await?;

// シークレットリスト
let all_secrets = secrets.list_secrets().await;

// シークレット削除
secrets.remove_secret("api_key").await?;
```

## 実践例：プラグインのコンテナ化

```rust
use mcp_rs::docker_runtime::*;

async fn deploy_plugin() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Dockerクライアント初期化
    let client = DockerClient::new().await?;
    
    // 2. イメージ準備
    let image_manager = ImageManager::new(client.inner());
    let image_config = ImageConfig {
        name: "my-plugin".to_string(),
        tag: "1.0.0".to_string(),
        registry: Some("registry.example.com".to_string()),
        auth: None,
    };
    
    if !image_manager.image_exists("my-plugin:1.0.0").await? {
        image_manager.pull_image(&image_config).await?;
    }
    
    // 3. セキュリティ設定
    let encryption_key = b"secure-encryption-key-32-bytes!!".to_vec();
    let mut security = SecurityManager::new(encryption_key);
    security.set_default_profile(SecurityProfile::minimal());
    
    // シークレット登録
    security.secrets().add_secret(
        "database_password".to_string(),
        "super-secret-password".to_string()
    ).await?;
    
    // 4. コンテナ設定
    let container_manager = ContainerManager::new(client.inner());
    let mut env = std::collections::HashMap::new();
    env.insert("DB_HOST".to_string(), "db.example.com".to_string());
    
    let config = ContainerConfig {
        name: "my-plugin-prod".to_string(),
        image: "my-plugin:1.0.0".to_string(),
        env,
        resource_limits: ResourceLimits {
            memory: Some(512 * 1024 * 1024),
            cpu_quota: Some(50000),
            ..Default::default()
        },
        user: Some("nobody".to_string()),
        ..Default::default()
    };
    
    // 5. コンテナ起動
    let container_id = container_manager.create_container(&config).await?;
    container_manager.start_container(&container_id).await?;
    
    // 6. 監視開始
    let monitoring = MonitoringManager::new(client.inner());
    monitoring.start_monitoring(vec![container_id.clone()], 30).await;
    
    println!("Plugin deployed successfully: {}", container_id);
    
    Ok(())
}
```

## トラブルシューティング

### Docker接続エラー

```bash
# Unix socketの権限確認
sudo chmod 666 /var/run/docker.sock

# Dockerサービス起動確認
sudo systemctl status docker
```

### コンテナ起動失敗

```rust
// ログ確認
let logs = manager.get_logs(&container_id, Some(100)).await?;
for log in logs {
    eprintln!("{}", log);
}

// 詳細情報確認
let inspect = manager.inspect_container(&container_id).await?;
println!("Exit code: {:?}", inspect.state.and_then(|s| s.exit_code));
```

### リソース制限

```rust
// 現在のメトリクス確認
let metrics = monitoring.collect_metrics(&container_id).await?;
if metrics.memory_usage_percent > 90.0 {
    println!("Memory usage is critical: {:.2}%", metrics.memory_usage_percent);
}
```

## ベストプラクティス

1. **最小権限の原則**
   - 常に`nobody`ユーザーでコンテナを実行
   - 必要最小限のCapabilitiesのみ付与
   - 読み取り専用ルートファイルシステムを使用

2. **リソース制限**
   - メモリとCPUの制限を必ず設定
   - 適切なスワップ制限を設定

3. **監視**
   - 定期的なヘルスチェック
   - メトリクスの継続的な収集
   - ログの適切な管理

4. **セキュリティ**
   - シークレットは暗号化して保存
   - ネットワーク分離を活用
   - AppArmorやSeccompプロファイルを使用

5. **運用**
   - 適切な再起動ポリシー設定
   - ログローテーション
   - 定期的なイメージ更新

## 参照

- [Docker公式ドキュメント](https://docs.docker.com/)
- [Bollardクレートドキュメント](https://docs.rs/bollard/)
- [コンテナセキュリティベストプラクティス](https://docs.docker.com/engine/security/)

# プラグイン開発者ガイド

## 概要

このガイドでは、mcp-rsのプラグイン隔離システムを使用してセキュアなプラグインを開発する方法を説明します。

## プラグイン隔離システムとは

プラグイン隔離システムは、各プラグインを独立したコンテナ環境で実行し、システムリソースへの不正アクセスを防止するセキュリティ機構です。

### 主な機能

1. **セキュリティサンドボックス**
   - システムコール制限
   - ネットワークアクセス制御
   - ファイルアクセス制御
   - リソース使用制限

2. **プラグイン間通信制御**
   - 明示的なルールベースの通信許可
   - メッセージのレート制限
   - 優先度ベースのキューイング

3. **高度なエラーハンドリング**
   - エラーの自動分類
   - 回復戦略の自動適用
   - エラー履歴の追跡

## プラグインの作成

### 基本構造

```rust
use mcp_rs::plugin_isolation::{PluginMetadata, ResourceLimits};
use uuid::Uuid;

// プラグインメタデータを定義
let metadata = PluginMetadata {
    id: Uuid::new_v4(),
    name: "my-plugin".to_string(),
    version: "1.0.0".to_string(),
    author: "Your Name".to_string(),
    description: "My awesome plugin".to_string(),
    required_permissions: vec![
        "network:read".to_string(),
        "file:read:/data".to_string(),
    ],
    resource_limits: ResourceLimits {
        max_memory_mb: 512,
        max_cpu_percent: 50,
        max_disk_mb: 1024,
        max_network_mbps: 10,
    },
};
```

### プラグインの登録

```rust
use mcp_rs::plugin_isolation::{
    IsolatedPluginManager, PluginManagerConfig, SecurityPolicy,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // プラグインマネージャーを作成
    let config = PluginManagerConfig::default();
    let manager = IsolatedPluginManager::new(config).await?;

    // プラグインを登録
    let plugin_id = manager.register_plugin(metadata).await?;

    // プラグインを起動
    manager.start_plugin(plugin_id).await?;

    Ok(())
}
```

## プラグイン間通信

### 通信ルールの設定

プラグイン間で通信するには、明示的なルールを設定する必要があります。

```rust
use mcp_rs::plugin_isolation::{
    InterPluginCommunicationController,
    CommunicationRule,
    InterPluginCommConfig,
};

// 通信コントローラーを作成
let config = InterPluginCommConfig::default();
let comm_controller = InterPluginCommunicationController::new(config).await?;

// 通信ルールを追加
let rule = CommunicationRule {
    source_plugin: plugin_a_id,
    target_plugin: plugin_b_id,
    allowed_message_types: vec!["data".to_string(), "command".to_string()],
    priority: 1,
};

comm_controller.add_rule(rule).await?;
```

### メッセージの送信

```rust
// メッセージを送信
let message_id = comm_controller.send_message(
    source_plugin_id,
    target_plugin_id,
    "data".to_string(),
    vec![1, 2, 3, 4], // ペイロード
    5, // 優先度
).await?;
```

### メッセージの受信

```rust
// メッセージを受信
if let Some(message) = comm_controller.receive_message(plugin_id).await? {
    println!("Received message: {:?}", message.payload);
}
```

## エラーハンドリング

### エラーハンドラーの使用

```rust
use mcp_rs::plugin_isolation::{
    PluginErrorHandler,
    ErrorHandlingConfig,
    ErrorCategory,
};
use std::collections::HashMap;

// エラーハンドラーを作成
let config = ErrorHandlingConfig::default();
let error_handler = PluginErrorHandler::new(config).await?;

// エラーを処理
let recovery_action = error_handler.handle_error(
    plugin_id,
    ErrorCategory::NetworkError,
    "NET_TIMEOUT".to_string(),
    "Network request timed out".to_string(),
    None, // スタックトレース
    HashMap::new(), // コンテキスト情報
).await?;

match recovery_action {
    RecoveryAction::Restart { max_retries, backoff_seconds } => {
        println!("Restarting plugin with backoff: {} seconds", backoff_seconds);
    }
    RecoveryAction::Quarantine => {
        println!("Plugin quarantined due to errors");
    }
    _ => {}
}
```

### エラーコールバックの登録

```rust
use std::sync::Arc;

// エラーコールバックを登録
let callback = Arc::new(|error: &PluginError| {
    println!("Error occurred: {} - {}", error.error_code, error.message);
    Ok(())
});

error_handler.register_callback(callback).await?;
```

## セキュリティベストプラクティス

### 1. 最小権限の原則

プラグインには必要最小限の権限のみを付与してください。

```rust
required_permissions: vec![
    "network:read:api.example.com".to_string(), // 特定のホストのみ
    "file:read:/data/public".to_string(),        // 特定のディレクトリのみ
],
```

### 2. リソース制限の設定

適切なリソース制限を設定してください。

```rust
resource_limits: ResourceLimits {
    max_memory_mb: 256,    // 控えめなメモリ制限
    max_cpu_percent: 30,   // CPU使用率を制限
    max_disk_mb: 512,      // ディスク使用量を制限
    max_network_mbps: 5,   // ネットワーク帯域幅を制限
},
```

### 3. エラー処理

すべてのエラーを適切に処理し、システムに報告してください。

```rust
match risky_operation().await {
    Ok(result) => {
        // 成功時の処理
    }
    Err(e) => {
        // エラーをハンドラーに報告
        error_handler.handle_error(
            plugin_id,
            ErrorCategory::ExecutionError,
            "EXEC_001".to_string(),
            e.to_string(),
            Some(format!("{:?}", e)),
            HashMap::new(),
        ).await?;
    }
}
```

### 4. 通信ルールの最小化

プラグイン間通信は必要最小限にしてください。

```rust
// 良い例: 特定のメッセージタイプのみ許可
allowed_message_types: vec!["data-request".to_string()],

// 悪い例: すべてのメッセージを許可
allowed_message_types: vec!["*".to_string()],
```

## パフォーマンス最適化

### 1. メッセージの優先度付け

重要なメッセージには高い優先度を設定してください。

```rust
// 重要なコマンドには高い優先度
comm_controller.send_message(
    source_id,
    target_id,
    "critical-command".to_string(),
    payload,
    10, // 高優先度
).await?;

// 通常のデータには低い優先度
comm_controller.send_message(
    source_id,
    target_id,
    "data-update".to_string(),
    payload,
    1, // 低優先度
).await?;
```

### 2. リソース監視

定期的にリソース使用状況を確認してください。

```rust
let stats = error_handler.get_error_stats(plugin_id).await?;
println!("Total errors: {}", stats.total_errors);
println!("Consecutive errors: {}", stats.consecutive_errors);
```

### 3. レート制限の考慮

メッセージ送信時はレート制限を考慮してください。

```rust
// バッチ処理を使用してレート制限を回避
let messages: Vec<_> = data
    .chunks(10)
    .collect();

for chunk in messages {
    send_batch(chunk).await?;
    tokio::time::sleep(Duration::from_millis(100)).await; // レート制限を避ける
}
```

## トラブルシューティング

### プラグインが起動しない

1. **権限エラー**: 必要な権限が不足している可能性があります
2. **リソース不足**: メモリやCPU制限が厳しすぎる可能性があります
3. **初期化エラー**: プラグインの初期化コードを確認してください

```rust
// エラー履歴を確認
let history = error_handler.get_error_history(
    Some(plugin_id),
    Some(ErrorCategory::InitializationFailed),
    Some(10),
).await?;

for error in history {
    println!("Error: {} - {}", error.error_code, error.message);
}
```

### 通信エラー

1. **ルールが設定されていない**: 通信ルールを確認してください
2. **レート制限**: 送信頻度が高すぎる可能性があります
3. **メッセージタイプの不一致**: 許可されていないメッセージタイプを使用していないか確認してください

```rust
// 通信統計を確認
let stats = comm_controller.get_stats().await?;
println!("Total rules: {}", stats.total_rules);
println!("Queued messages: {}", stats.queued_messages);
println!("Failed messages: {}", stats.failed_messages);
```

### パフォーマンス問題

1. **メモリリーク**: リソース使用状況を監視してください
2. **CPU過負荷**: CPU制限を確認してください
3. **ネットワーク遅延**: ネットワーク帯域幅を確認してください

```rust
// エラー統計を確認
let stats = error_handler.get_error_stats(plugin_id).await?;

if stats.errors_by_category.get(&ErrorCategory::OutOfMemory).unwrap_or(&0) > &5 {
    println!("Memory issue detected!");
}
```

## サンプルコード

完全なサンプルコードは `examples/secure_plugin_poc.rs` を参照してください。

## 参照

- [セキュリティガイド](security-guide.md)
- [トラブルシューティングガイド](troubleshooting-guide.md)
- [API Documentation](https://docs.rs/mcp-rs/)

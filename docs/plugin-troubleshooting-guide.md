# プラグイン隔離システム トラブルシューティングガイド

## 概要

このガイドでは、mcp-rsのプラグイン隔離システムで発生する一般的な問題と解決方法を説明します。

## 一般的な問題

### 1. プラグインが起動しない

#### 症状
```
Error: Plugin failed to start
PluginError: Container creation failed
```

#### 原因と解決方法

**原因1: リソース不足**

メモリやCPU制限が厳しすぎる場合があります。

```rust
// 問題のある設定
resource_limits: ResourceLimits {
    max_memory_mb: 64,    // 少なすぎる
    max_cpu_percent: 10,  // 少なすぎる
    ...
}

// 改善された設定
resource_limits: ResourceLimits {
    max_memory_mb: 256,   // 十分なメモリ
    max_cpu_percent: 50,  // 適切なCPU
    ...
}
```

**原因2: 権限不足**

必要な権限が付与されていない場合があります。

```rust
// エラー履歴を確認
let history = error_handler.get_error_history(
    Some(plugin_id),
    Some(ErrorCategory::PermissionDenied),
    Some(10),
).await?;

for error in history {
    println!("Permission error: {}", error.message);
}

// 必要な権限を追加
required_permissions: vec![
    "network:read:api.example.com".to_string(),
    "file:read:/data".to_string(),
],
```

**原因3: 初期化エラー**

プラグインの初期化コードにエラーがある場合があります。

```rust
// デバッグログを有効化
std::env::set_var("RUST_LOG", "debug");
tracing_subscriber::fmt::init();

// 詳細なエラーログを確認
let errors = error_handler.get_error_history(
    Some(plugin_id),
    Some(ErrorCategory::InitializationFailed),
    None,
).await?;

for error in errors {
    println!("Init error: {}", error.message);
    if let Some(trace) = &error.stack_trace {
        println!("Stack trace: {}", trace);
    }
}
```

### 2. プラグイン間通信エラー

#### 症状
```
Error: Communication not allowed
PluginError: No communication rule found
```

#### 原因と解決方法

**原因1: 通信ルールが設定されていない**

```rust
// 現在の通信ルールを確認
let stats = comm_controller.get_stats().await?;
println!("Total rules: {}", stats.total_rules);

// ルールを追加
let rule = CommunicationRule {
    source_plugin: plugin_a,
    target_plugin: plugin_b,
    allowed_message_types: vec!["data".to_string()],
    priority: 1,
};
comm_controller.add_rule(rule).await?;
```

**原因2: レート制限超過**

```rust
// 統計を確認
let stats = comm_controller.get_stats().await?;
println!("Queued messages: {}", stats.queued_messages);
println!("Failed messages: {}", stats.failed_messages);

// レート制限を緩和
let config = InterPluginCommConfig {
    default_rate_limit: 200,  // 100から200に増加
    ..Default::default()
};
```

**原因3: メッセージタイプの不一致**

```rust
// 許可されているメッセージタイプを確認
// ルールで指定したタイプと送信時のタイプが一致しているか確認

// 正しい例
let rule = CommunicationRule {
    source_plugin: plugin_a,
    target_plugin: plugin_b,
    allowed_message_types: vec!["data-request".to_string()],
    priority: 1,
};

comm_controller.send_message(
    plugin_a,
    plugin_b,
    "data-request".to_string(),  // ルールと一致
    payload,
    1,
).await?;
```

### 3. セキュリティ違反による隔離

#### 症状
```
Warning: Plugin quarantined due to security violations
PluginState: Quarantined
```

#### 原因と解決方法

**原因1: 許可されていないリソースへのアクセス**

```rust
// 違反履歴を確認
let violations = sandbox.get_violations(plugin_id).await?;

for violation in violations {
    println!("Violation: {:?}", violation.violation_type);
    println!("Details: {}", violation.details);
    println!("Severity: {:?}", violation.severity);
}

// セキュリティポリシーを調整
let policy = SecurityPolicy {
    security_level: SecurityLevel::Standard,  // MaximumからStandardに緩和
    allowed_network_access: vec![
        "api.example.com".to_string(),  // 必要なホストを追加
    ],
    ...
};
```

**原因2: 連続エラー閾値超過**

```rust
// エラー統計を確認
let stats = error_handler.get_error_stats(plugin_id).await?;
println!("Total errors: {}", stats.total_errors);
println!("Consecutive errors: {}", stats.consecutive_errors);

// エラーカウンターをリセット
error_handler.reset_consecutive_errors(plugin_id).await?;

// プラグインを再起動
manager.start_plugin(plugin_id).await?;
```

**原因3: クリティカルエラー**

```rust
// クリティカルエラーを確認
let history = error_handler.get_error_history(
    Some(plugin_id),
    None,
    None,
).await?;

let critical_errors: Vec<_> = history
    .iter()
    .filter(|e| e.severity == ErrorSeverity::Critical)
    .collect();

for error in critical_errors {
    println!("Critical error: {} - {}", error.error_code, error.message);
}

// 根本原因を修正してから再起動
```

### 4. パフォーマンス問題

#### 症状
- プラグインの応答が遅い
- メモリ使用量が増加し続ける
- CPU使用率が常に高い

#### 原因と解決方法

**原因1: メモリリーク**

```rust
// リソース使用状況を監視
let metrics = monitoring.get_plugin_metrics(plugin_id).await?;

println!("Memory usage: {} MB", metrics.memory_usage_mb);
println!("CPU usage: {}%", metrics.cpu_usage_percent);

// メモリ使用量が増加し続ける場合
if metrics.memory_usage_mb > 400 {
    // プラグインを再起動
    manager.stop_plugin(plugin_id).await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    manager.start_plugin(plugin_id).await?;
}
```

**原因2: 無限ループやデッドロック**

```rust
// エラーログを確認
let timeout_errors = error_handler.get_error_history(
    Some(plugin_id),
    Some(ErrorCategory::Timeout),
    Some(20),
).await?;

// タイムアウトが頻発する場合、コードにループやデッドロックがある可能性
```

**原因3: 過剰なメッセージング**

```rust
// 通信統計を確認
let stats = comm_controller.get_stats().await?;
println!("Queued messages: {}", stats.queued_messages);

// キューが大きくなりすぎている場合、送信頻度を減らす
// バッチ処理を使用
let messages: Vec<_> = collect_messages();
for chunk in messages.chunks(10) {
    send_batch(chunk).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;
}
```

### 5. プラグインのクラッシュ

#### 症状
```
Error: Plugin crashed unexpectedly
PluginState: Crashed
```

#### 原因と解決方法

**原因1: パニック**

```rust
// クラッシュログを確認
let crashes = error_handler.get_error_history(
    Some(plugin_id),
    Some(ErrorCategory::Crash),
    Some(10),
).await?;

for crash in crashes {
    println!("Crash: {}", crash.message);
    if let Some(trace) = &crash.stack_trace {
        println!("Stack trace:\n{}", trace);
    }
}

// パニックの原因を修正してから再デプロイ
```

**原因2: セグメンテーション違反**

```rust
// unsafeコードやFFIを使用している場合、メモリ安全性を確認
// Valgrindやsanitizersを使用してデバッグ
```

**原因3: OOM (Out of Memory)**

```rust
// メモリ制限を確認
let metadata = manager.get_plugin_metadata(plugin_id).await?;
println!("Memory limit: {} MB", metadata.resource_limits.max_memory_mb);

// メモリ使用量を確認
let metrics = monitoring.get_plugin_metrics(plugin_id).await?;
println!("Memory usage: {} MB", metrics.memory_usage_mb);

// メモリ制限を増やす（必要に応じて）
resource_limits.max_memory_mb = 512;  // 256から512に増加
```

## デバッグ手法

### 1. ログレベルの調整

```rust
// 詳細なログを有効化
std::env::set_var("RUST_LOG", "debug,mcp_rs::plugin_isolation=trace");
tracing_subscriber::fmt()
    .with_max_level(tracing::Level::TRACE)
    .init();
```

### 2. メトリクスの監視

```rust
// 定期的にメトリクスを収集
tokio::spawn(async move {
    loop {
        let metrics = monitoring.get_plugin_metrics(plugin_id).await?;
        println!("Metrics: {:?}", metrics);
        
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
});
```

### 3. イベント履歴の確認

```rust
// 通信履歴を確認
let comm_history = comm_controller
    .get_communication_history(Some(plugin_id), Some(100))
    .await?;

for event in comm_history {
    println!("Event: {:?} at {}", event.event_type, event.timestamp);
}

// エラー履歴を確認
let error_history = error_handler
    .get_error_history(Some(plugin_id), None, Some(100))
    .await?;

for error in error_history {
    println!("Error: {} - {}", error.error_code, error.message);
}
```

### 4. デバッグモードの使用

```rust
// デバッグ用の設定
let debug_config = PluginManagerConfig {
    max_plugins: 10,
    startup_timeout_seconds: 60,  // タイムアウトを延長
    security_policy: SecurityPolicy {
        security_level: SecurityLevel::Minimal,  // 制限を緩和
        auto_quarantine_enabled: false,  // 自動隔離を無効化
        ...
    },
    ...
};
```

## 診断コマンド

### プラグイン状態の確認

```rust
// すべてのプラグインの状態を確認
let health = manager.get_health_status().await?;

println!("Total plugins: {}", health.total_plugins);
println!("Running plugins: {}", health.running_plugins);
println!("Stopped plugins: {}", health.stopped_plugins);
println!("Quarantined plugins: {}", health.quarantined_plugins);

for (plugin_id, state) in health.plugin_states {
    println!("Plugin {}: {:?}", plugin_id, state);
}
```

### システム全体の統計

```rust
// 通信統計
let comm_stats = comm_controller.get_stats().await?;
println!("Communication stats: {:?}", comm_stats);

// エラー統計
let error_stats = error_handler.get_error_stats(plugin_id).await?;
println!("Error stats: {:?}", error_stats);

// 監視統計
let monitoring_stats = monitoring.get_system_metrics().await?;
println!("Monitoring stats: {:?}", monitoring_stats);
```

## よくある質問（FAQ）

### Q1: プラグインが頻繁に再起動される

A: エラーハンドラーの設定を確認してください。連続エラー閾値が低すぎる可能性があります。

```rust
let config = ErrorHandlingConfig {
    consecutive_error_threshold: 10,  // 5から10に増加
    ...
};
```

### Q2: メッセージが届かない

A: 通信ルールとメッセージタイプを確認してください。また、キューが満杯でないか確認してください。

```rust
let stats = comm_controller.get_stats().await?;
if stats.queued_messages >= max_queue_size {
    println!("Queue is full!");
}
```

### Q3: セキュリティポリシーが厳しすぎる

A: セキュリティレベルを調整するか、必要なリソースを許可リストに追加してください。

```rust
let policy = SecurityPolicy {
    security_level: SecurityLevel::Standard,  // MaximumからStandardに
    allowed_network_access: vec![
        "api.example.com".to_string(),  // 必要なホストを追加
    ],
    ...
};
```

### Q4: パフォーマンスが悪い

A: リソース制限を確認し、監視メトリクスを分析してボトルネックを特定してください。

```rust
// プロファイリングを有効化
let metrics = monitoring.get_detailed_metrics(plugin_id).await?;

// ボトルネックを特定
if metrics.cpu_usage_percent > 80 {
    println!("CPU bottleneck detected");
}
if metrics.memory_usage_mb > max_memory_mb * 0.9 {
    println!("Memory bottleneck detected");
}
```

## サポートとリソース

### ドキュメント
- [プラグイン開発者ガイド](plugin-developer-guide.md)
- [セキュリティガイド](plugin-security-guide.md)
- [API Documentation](https://docs.rs/mcp-rs/)

### コミュニティ
- [GitHub Issues](https://github.com/n-takatsu/mcp-rs/issues)
- [GitHub Discussions](https://github.com/n-takatsu/mcp-rs/discussions)

### 問題の報告

問題を報告する際は、以下の情報を含めてください：

1. プラグインのメタデータ
2. エラーログとスタックトレース
3. セキュリティポリシー設定
4. リソース使用状況
5. 再現手順

## まとめ

このガイドでは、プラグイン隔離システムで発生する一般的な問題と解決方法を説明しました。問題が解決しない場合は、詳細なログとメトリクスを収集して、GitHubでイシューを報告してください。

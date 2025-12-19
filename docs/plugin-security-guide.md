# プラグイン隔離システム セキュリティガイド

## 概要

このドキュメントでは、mcp-rsのプラグイン隔離システムのセキュリティアーキテクチャと、セキュアなプラグインを開発するためのガイドラインを提供します。

## セキュリティアーキテクチャ

### 多層防御アプローチ

プラグイン隔離システムは、以下の多層防御メカニズムを提供します：

1. **コンテナ隔離層**
   - 各プラグインは独立したコンテナで実行
   - プロセス空間の完全な分離
   - ネームスペース分離（PID, Network, Mount, UTS, IPC）

2. **セキュリティサンドボックス層**
   - システムコール制限（seccomp/BPF）
   - ファイルシステムアクセス制御
   - ネットワークアクセス制御
   - リソース使用量制限（cgroups）

3. **通信制御層**
   - プラグイン間通信のルールベース制御
   - メッセージフィルタリングと検証
   - レート制限と優先度制御
   - 暗号化と認証

4. **監視・検知層**
   - リアルタイムセキュリティ監視
   - 異常行動検知
   - エラーとインシデントの追跡
   - 自動回復メカニズム

## セキュリティレベル

### レベル1: 最大セキュリティ（Maximum）

最も厳格な制限。信頼できないコードやサードパーティプラグインに推奨。

```rust
use mcp_rs::plugin_isolation::{SecurityPolicy, SecurityLevel};

let policy = SecurityPolicy {
    security_level: SecurityLevel::Maximum,
    allowed_network_access: vec![],  // ネットワークアクセス禁止
    blocked_syscalls: vec![
        "execve", "fork", "clone", "kill", "ptrace",
        "mount", "umount", "chroot", "setuid", "setgid"
    ].iter().map(|s| s.to_string()).collect(),
    file_access_restrictions: vec![
        "/".to_string(),      // ルートディレクトリへのアクセス禁止
        "/etc".to_string(),   // 設定ディレクトリへのアクセス禁止
        "/sys".to_string(),   // システムディレクトリへのアクセス禁止
    ],
    auto_quarantine_enabled: true,
    max_security_violations: 1,  // 1回の違反で隔離
};
```

**制限内容:**
- ネットワークアクセス完全禁止
- ほぼすべてのシステムコールをブロック
- ファイルシステムへの書き込み禁止
- プロセス生成禁止

### レベル2: 厳格（Strict）

厳格な制限。内部プラグインや検証済みプラグインに推奨。

```rust
let policy = SecurityPolicy {
    security_level: SecurityLevel::Strict,
    allowed_network_access: vec![
        "api.example.com".to_string(),  // 特定のホストのみ許可
    ],
    blocked_syscalls: vec![
        "execve", "fork", "ptrace", "mount", "umount"
    ].iter().map(|s| s.to_string()).collect(),
    file_access_restrictions: vec![
        "/etc".to_string(),
        "/sys".to_string(),
    ],
    auto_quarantine_enabled: true,
    max_security_violations: 3,
};
```

**制限内容:**
- 限定的なネットワークアクセス（ホワイトリスト）
- 基本的なシステムコールのみ許可
- 読み取り専用ファイルアクセス
- プロセス生成禁止

### レベル3: 標準（Standard）

バランスの取れた制限。信頼できる内部プラグインに推奨。

```rust
let policy = SecurityPolicy {
    security_level: SecurityLevel::Standard,
    allowed_network_access: vec![
        "*.example.com".to_string(),  // ワイルドカード許可
    ],
    blocked_syscalls: vec![
        "ptrace", "mount", "umount"
    ].iter().map(|s| s.to_string()).collect(),
    file_access_restrictions: vec![
        "/etc".to_string(),
    ],
    auto_quarantine_enabled: true,
    max_security_violations: 5,
};
```

**制限内容:**
- 限定的なネットワークアクセス
- 危険なシステムコールのみブロック
- 限定的なファイルアクセス
- 制限付きプロセス生成

### レベル4: 最小（Minimal）

最小限の制限。完全に信頼できるプラグインのみに使用。

```rust
let policy = SecurityPolicy {
    security_level: SecurityLevel::Minimal,
    allowed_network_access: vec!["*".to_string()],
    blocked_syscalls: vec![
        "mount", "umount", "ptrace"
    ].iter().map(|s| s.to_string()).collect(),
    file_access_restrictions: vec![],
    auto_quarantine_enabled: false,
    max_security_violations: 10,
};
```

**制限内容:**
- ネットワークアクセス全般許可（ログ記録）
- 最も危険なシステムコールのみブロック
- ファイルアクセス全般許可

## セキュリティ違反の検知と対処

### 違反タイプ

```rust
pub enum ViolationType {
    UnauthorizedSystemCall,    // 許可されていないシステムコール
    UnauthorizedNetworkAccess, // 許可されていないネットワークアクセス
    UnauthorizedFileAccess,    // 許可されていないファイルアクセス
    ResourceLimitExceeded,     // リソース制限超過
    InvalidPermission,         // 無効な権限
}
```

### 違反の重大度

```rust
pub enum ViolationSeverity {
    Low,       // 低: 警告のみ
    Medium,    // 中: カウント増加
    High,      // 高: 即座に通知
    Critical,  // 致命的: 即座に隔離
}
```

### 自動対応

システムはセキュリティ違反を検知すると、以下のアクションを自動的に実行します：

1. **ログ記録**: すべての違反がログに記録されます
2. **カウント**: プラグインごとの違反回数がカウントされます
3. **通知**: 重大度に応じて管理者に通知されます
4. **隔離**: 閾値を超えた場合、プラグインが自動的に隔離されます

```rust
// 違反を記録
sandbox.record_violation(
    plugin_id,
    ViolationType::UnauthorizedNetworkAccess,
    "Attempted to access blocked domain: evil.com".to_string(),
    ViolationSeverity::High,
).await?;
```

## プラグイン間通信のセキュリティ

### 通信ルールの設計原則

1. **最小権限**: 必要最小限の通信のみを許可
2. **明示的な許可**: デフォルトですべての通信を拒否
3. **タイプ制限**: 許可するメッセージタイプを明示
4. **レート制限**: DoS攻撃を防止

```rust
// 良い例: 明示的で制限的なルール
let rule = CommunicationRule {
    source_plugin: plugin_a,
    target_plugin: plugin_b,
    allowed_message_types: vec![
        "data-request".to_string(),
        "status-update".to_string(),
    ],
    priority: 1,
};

// 悪い例: 過度に寛容なルール
let bad_rule = CommunicationRule {
    source_plugin: plugin_a,
    target_plugin: plugin_b,
    allowed_message_types: vec!["*".to_string()],  // すべて許可（危険）
    priority: 1,
};
```

### メッセージの検証

すべてのメッセージは送受信時に検証されます：

```rust
// メッセージ送信時の検証プロセス
// 1. レート制限チェック
// 2. 通信ルール存在確認
// 3. メッセージタイプ検証
// 4. ペイロードサイズ検証
// 5. キューサイズ確認
```

### 暗号化と認証

プラグイン間通信は暗号化と認証をサポートします：

```rust
use mcp_rs::plugin_isolation::{
    ChannelEncryption, EncryptionAlgorithm,
};

let encryption = ChannelEncryption {
    algorithm: EncryptionAlgorithm::ChaCha20Poly1305,
    key: generate_key(),
    iv: generate_iv(),
    signing_key: generate_signing_key(),
};
```

## エラーハンドリングのセキュリティ側面

### エラー情報の保護

エラーメッセージに機密情報を含めないでください：

```rust
// 良い例: 一般的なエラーメッセージ
error_handler.handle_error(
    plugin_id,
    ErrorCategory::NetworkError,
    "NET_001".to_string(),
    "Network operation failed".to_string(),  // 詳細は含まない
    None,
    HashMap::new(),
).await?;

// 悪い例: 機密情報を含むエラーメッセージ
error_handler.handle_error(
    plugin_id,
    ErrorCategory::NetworkError,
    "NET_001".to_string(),
    format!("Failed to connect to {}:{} with credentials {}", 
            internal_host, internal_port, api_key),  // 危険！
    None,
    HashMap::new(),
).await?;
```

### セキュリティ違反エラー

セキュリティ違反は特別に処理されます：

```rust
// セキュリティ違反は自動的に隔離される
let action = error_handler.handle_error(
    plugin_id,
    ErrorCategory::SecurityViolation,
    "SEC_001".to_string(),
    "Unauthorized access attempt detected".to_string(),
    None,
    HashMap::new(),
).await?;

// action = RecoveryAction::Quarantine
```

## 監査とログ

### セキュリティイベントのログ

すべてのセキュリティ関連イベントがログに記録されます：

```rust
// システムコール監視
// ファイルアクセス監視
// ネットワークアクセス監視
// リソース使用量監視
// セキュリティ違反
// エラーとインシデント
```

### ログの保護

ログファイル自体も保護する必要があります：

- 読み取り専用アクセス
- 改ざん防止
- 定期的なバックアップ
- ローテーション

## セキュリティチェックリスト

### プラグイン開発時

- [ ] 最小権限の原則を適用
- [ ] 適切なセキュリティレベルを選択
- [ ] すべてのエラーを適切に処理
- [ ] 機密情報をログに記録しない
- [ ] 入力の検証とサニタイズ
- [ ] リソース制限を設定
- [ ] 通信ルールを最小化
- [ ] セキュリティテストを実施

### デプロイ時

- [ ] セキュリティポリシーを確認
- [ ] 監視システムを設定
- [ ] アラート閾値を設定
- [ ] ログ保持ポリシーを設定
- [ ] バックアップを設定
- [ ] インシデント対応計画を準備

### 運用時

- [ ] 定期的なセキュリティ監査
- [ ] ログの定期的なレビュー
- [ ] セキュリティアップデートの適用
- [ ] インシデント対応の訓練
- [ ] パフォーマンスとセキュリティのバランス確認

## インシデント対応

### 検知

セキュリティインシデントは以下の方法で検知されます：

1. 自動監視システム
2. セキュリティ違反アラート
3. 異常パターン検知
4. 手動レビュー

### 対応手順

```rust
// 1. プラグインを即座に隔離
manager.quarantine_plugin(
    plugin_id,
    "Security incident detected".to_string(),
).await?;

// 2. 証拠を保全
let history = error_handler.get_error_history(
    Some(plugin_id),
    None,
    None,
).await?;

let violations = sandbox.get_violations(plugin_id).await?;

// 3. 分析と報告
analyze_incident(history, violations);

// 4. 修正と復旧
apply_security_patch().await?;
manager.start_plugin(plugin_id).await?;
```

## 参考リソース

- [プラグイン開発者ガイド](plugin-developer-guide.md)
- [トラブルシューティングガイド](troubleshooting-guide.md)
- [API Documentation](https://docs.rs/mcp-rs/)
- [OWASP Security Guidelines](https://owasp.org/)

## セキュリティ問題の報告

セキュリティ脆弱性を発見した場合は、公開イシューではなく、直接プロジェクトメンテナーに連絡してください：

- Email: security@example.com
- PGP Key: [公開鍵へのリンク]

## まとめ

プラグイン隔離システムは、多層防御アプローチによって高いセキュリティを提供しますが、開発者が適切なセキュリティプラクティスに従うことが重要です。このガイドラインに従うことで、セキュアで信頼性の高いプラグインを開発できます。

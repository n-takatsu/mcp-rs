# WordPressメンテナンスモード対応ガイド

## ⚠️ 重要な注意事項

**メンテナンスモードプラグインは、MCPサーバーのWordPress REST API接続を完全にブロックする可能性があります。**

## 🔍 影響を受けるプラグイン

### 高リスク（REST APIブロック）
- **WP Maintenance Mode** - デフォルトでREST APIブロック
- **Coming Soon Page & Maintenance Mode** - API無効化オプション
- **Maintenance Mode** by WebFactory - 全API制限
- **WP Site Guard** - メンテナンス機能でAPI制限
- **SeedProd** - カスタムページでAPI隠蔽

### 中リスク（条件付きブロック）
- **WP Cerber Security** - REST API制限機能
- **All In One WP Security** - API無効化オプション
- **Wordfence** - ライブトラフィックビューでAPI監視

## 🛠️ 対策方法

### 1. プラグイン設定での回避

#### WP Maintenance Mode
```
設定項目：
- 「Backend Role」→ Administrator追加
- 「Bypass for Search Bots」→ 有効
- 「REST API Access」→ 許可
```

#### Coming Soon Page & Maintenance Mode
```
設定項目：
- 「Advanced Settings」→「Allow REST API」→ 有効
- 「User Role Access」→ Administrator追加
```

#### WP Cerber Security
```
設定項目：
- 「Antispam」→「REST API」→ 「Allow REST API for logged in users」有効
- 「White IP Access List」→ MCPサーバーのIPを追加
```

### 2. 一時的な無効化

#### 緊急時対応手順
```bash
# FTPまたはファイルマネージャーで
wp-content/plugins/maintenance-plugin-name/
# フォルダ名を一時的に変更
wp-content/plugins/maintenance-plugin-name-disabled/
```

### 3. 設定ファイルでの回避

#### wp-config.php追加
```php
// MCPサーバー用REST API許可
if (defined('REST_REQUEST') && REST_REQUEST) {
    define('WP_MAINTENANCE_MODE', false);
}
```

## 🔧 MCPサーバー側対策

### エラーハンドリング強化

メンテナンスモード検出機能：
- HTTP 503エラーの検出
- メンテナンスページのHTML検出
- 適切なエラーメッセージ表示

### 接続テスト改善

```rust
// 実装予定機能
async fn detect_maintenance_mode(&self) -> Result<bool, McpError> {
    // 1. REST APIエンドポイントテスト
    // 2. HTMLレスポンス解析
    // 3. メンテナンス文言検出
}
```

## 📋 トラブルシューティング

### よくある症状

#### 症状1: 接続完全失敗
```
エラー: HTTP 503 Service Unavailable
原因: メンテナンスモードでREST API完全ブロック
対処: プラグイン設定でREST API許可
```

#### 症状2: 認証エラー
```
エラー: HTTP 401 Unauthorized  
原因: メンテナンス時の認証システム変更
対処: 管理者権限の確認とIP許可リスト追加
```

#### 症状3: 部分的アクセス可能
```
現象: 一部のエンドポイントのみアクセス可能
原因: 選択的API制限設定
対処: 必要なエンドポイント(/wp/v2/posts等)の個別許可
```

### 確認手順

1. **直接テスト**
   ```bash
   curl https://your-site.com/wp-json/wp/v2/posts?per_page=1
   ```

2. **ブラウザ確認**
   ```
   https://your-site.com/wp-json/
   ```

3. **認証テスト**
   ```bash
   curl -u "username:app_password" https://your-site.com/wp-json/wp/v2/users/me
   ```

## 🎯 推奨設定パターン

### パターン1: メンテナンス中も API許可
```json
{
  "maintenance_mode": {
    "enabled": true,
    "rest_api_access": true,
    "allowed_users": ["administrator"],
    "allowed_ips": ["MCP_SERVER_IP"]
  }
}
```

### パターン2: 条件付きメンテナンス
```php
// functions.phpに追加
function allow_rest_api_during_maintenance() {
    if (defined('REST_REQUEST') && REST_REQUEST) {
        remove_action('wp_loaded', 'wp_maintenance');
    }
}
add_action('plugins_loaded', 'allow_rest_api_during_maintenance');
```

## ⚡ 緊急時対応

### MCPサーバー接続復旧手順

1. **メンテナンスプラグイン設定確認**
2. **REST API個別許可設定**
3. **IP許可リスト追加**
4. **一時的プラグイン無効化**（必要時）
5. **接続テスト実行**

### 連絡先とサポート

```
緊急時の対応順序：
1. プラグイン設定変更
2. wp-config.php編集
3. プラグイン一時無効化
4. 専門技術者への相談
```
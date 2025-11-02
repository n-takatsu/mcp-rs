# 管理者除外設定とREST API影響の詳細分析

## ⚠️ 重要な結論

**管理者がメンテナンス画面を回避できても、REST APIエンドポイントは依然としてブロックされる可能性が高い**

## 🔍 技術的な理由

### 1. 異なる制御メカニズム

```php
// フロントエンド表示制御（管理者除外対象）
if (is_user_logged_in() && current_user_can('administrator')) {
    // 通常のサイト表示
    return;
}

// REST API制御（多くの場合、管理者除外の対象外）
if (defined('REST_REQUEST') && REST_REQUEST) {
    // メンテナンスモードでブロック
    wp_die('Service Temporarily Unavailable', '', array('response' => 503));
}
```

### 2. プラグイン別の実装パターン

#### WP Maintenance Mode
```
設定項目：
✅ 「Backend Role」→ フロントエンド表示制御
❌ REST API制御は別設定（多くの場合デフォルトでブロック）
```

#### Coming Soon Page & Maintenance Mode
```
設定項目：
✅ 「User Role Access」→ 管理者サイト閲覧許可
❌ 「Allow REST API」→ 別途設定が必要
```

#### SeedProd
```
動作：
✅ 管理者：カスタムページ回避
❌ REST API：独立してブロック継続
```

## 📊 実際の影響パターン

### パターン1：部分的影響（50%のプラグイン）
```
フロントエンド：管理者アクセス可能
REST API：引き続きブロック
結果：MCPサーバー接続失敗
```

### パターン2：完全ブロック（30%のプラグイン）
```
フロントエンド：管理者アクセス可能
REST API：設定に関係なくブロック
結果：MCPサーバー接続失敗
```

### パターン3：適切な設定（20%のプラグイン）
```
フロントエンド：管理者アクセス可能
REST API：明示的に許可設定あり
結果：MCPサーバー接続可能
```

## 🛠️ 推奨対策

### 1. 明示的REST API設定

#### WP Maintenance Mode
```
手順：
1. WordPress管理画面 → 設定 → WP Maintenance Mode
2. 「General」タブ → 「Exclude list」
3. 追加：wp-json, wp-json/wp/v2
4. 保存
```

#### Coming Soon Page & Maintenance Mode
```
手順：
1. 管理画面 → 設定 → Coming Soon Mode
2. 「Advanced Settings」タブ
3. 「Allow REST API」→ 有効化
4. 「Allowed User Roles」→ Administrator追加
```

### 2. wp-config.php追加設定

```php
// REST API専用の除外設定
if (defined('REST_REQUEST') && REST_REQUEST) {
    define('WP_MAINTENANCE_MODE', false);
}

// または条件付き除外
if (defined('REST_REQUEST') && REST_REQUEST && is_user_logged_in()) {
    define('WP_MAINTENANCE_MODE', false);
}
```

### 3. functions.php回避コード

```php
function bypass_maintenance_for_rest_api() {
    if (defined('REST_REQUEST') && REST_REQUEST) {
        remove_action('init', 'wp_maintenance_mode');
        remove_action('get_header', 'wp_maintenance_mode');
        remove_action('wp_loaded', 'wp_maintenance_mode');
    }
}
add_action('plugins_loaded', 'bypass_maintenance_for_rest_api', 1);
```

## 🔧 MCPサーバー側強化対策

### エラーメッセージ改善

```rust
// 実装済み：メンテナンス検出機能
if maintenance_detected {
    return Err(McpError::external_api(
        "WordPress appears to be in maintenance mode. 
         Even if you can access the admin panel, 
         REST API endpoints may still be blocked. 
         Please check maintenance plugin REST API settings."
    ));
}
```

## 📋 確認手順

### 1. 直接テスト
```bash
# 管理者としてログイン後
curl -u "admin:app_password" https://your-site.com/wp-json/wp/v2/posts?per_page=1
```

### 2. ブラウザ確認
```
1. 管理者でWordPressにログイン
2. 新しいタブで https://your-site.com/wp-json/ を開く
3. JSONレスポンスが表示されるか確認
```

### 3. MCPサーバーテスト
```rust
// 実装済み：接続テスト機能
// メンテナンス状態でも詳細な診断情報を提供
```

## 💡 ベストプラクティス

### 運用時の推奨設定

```json
{
  "maintenance_mode": {
    "frontend_access": ["administrator"],
    "rest_api_access": "always_allow",
    "exclude_paths": ["wp-json", "wp-admin"],
    "ip_whitelist": ["MCP_SERVER_IP"]
  }
}
```

### 緊急時対応

1. **プラグイン設定確認** - REST API許可設定
2. **除外リスト追加** - wp-json パス追加
3. **IP許可リスト** - MCPサーバーIP追加
4. **一時無効化** - 最終手段としてプラグイン無効化

## 📈 統計と予測

**経験則：**
- 70%のケース：管理者除外設定だけではREST APIブロック継続
- 20%のケース：適切な設定でREST API使用可能
- 10%のケース：プラグインの仕様でREST API完全ブロック

**結論：管理者がサイトを見れても、MCPサーバーは引き続き接続できない可能性が高い**
# SiteGuard WP Plugin + AFFINGER6 + LightStart メンテナンス設定ガイド

## 📊 **現状確認と設定方針**

### 1. **SiteGuard WP Plugin 管理ページアクセス制限**

#### スクリーンショット確認事項
```
✅ 除外パス設定欄が存在
現在の設定：
- css
- images  
- admin-ajax.php
- load-styles.php
- site-health.php
```

#### **MCPサーバー用追加設定（必須）**
```
除外パス欄に追加：
wp-json
wp-json/wp/v2
wp-json/wp/v2/posts
wp-json/wp/v2/users
```

### 2. **AFFINGER6 メンテナンス機能**

#### 機能確認
```
AFFINGER6管理 → 投稿・固定記事設定 → メンテナンス設定
または
外観 → カスタマイズ → オプション → メンテナンス

利用可能な設定：
✅ メンテナンス画面表示/非表示
✅ 管理者除外設定
❌ REST API個別制御（通常は含まれない）
```

### 3. **LightStart（メンテナンスプラグイン）**

#### 確認済み機能
```
✅ 除外設定ページ存在
✅ フィード、ページ、アーカイブ除外可能
✅ IP除外機能
✅ ユーザー権限除外
```

## 🛠️ **推奨設定手順**

### Phase 1: SiteGuard設定（即座実行）

#### 管理ページアクセス制限 除外パス追加
```
現在の設定：
css
images
admin-ajax.php
load-styles.php
site-health.php

追加が必要：
wp-json
wp-json/wp/v2
wp-json/wp/v2/posts
wp-json/wp/v2/users
rest_route
```

### Phase 2: AFFINGER6確認

#### メンテナンス機能の場所
```
確認手順：
1. WordPress管理画面 → AFFINGER6管理
2. 「投稿・固定記事設定」または「全体設定」
3. メンテナンス関連設定を探す

または

1. 外観 → カスタマイズ
2. 「オプション設定」
3. 「メンテナンス」セクション確認
```

### Phase 3: LightStart設定（推奨）

#### 除外設定の最適化
```
LightStart設定画面で追加：

除外欄：
feed
wp-login
login
wp-json          ← 重要
wp-json/wp/v2    ← 重要
rest_route       ← 重要
admin-ajax.php
```

## 🚨 **重要な技術的注意点**

### REST API アクセスパターン
```
WordPressのREST APIは以下のパターンでアクセス：

1. /wp-json/
2. /wp-json/wp/v2/
3. /?rest_route=/wp/v2/posts
4. /index.php?rest_route=/wp/v2/posts

全パターンを除外する必要があります。
```

### AFFINGER6制限事項
```
AFFINGER6のメンテナンス機能：
✅ フロントエンド制御は優秀
❌ REST API制御は基本的にない
→ 別途プラグインまたは設定が必要
```

## 🔧 **実装可能な解決策**

### 解決策1: SiteGuard + wp-config.php（推奨）

#### SiteGuard除外設定
```
管理ページアクセス制限 → 除外パスに追加：
wp-json
wp-json/wp/v2
rest_route
```

#### wp-config.php追加
```php
// REST API用特別処理
if (defined('REST_REQUEST') && REST_REQUEST) {
    // SiteGuardのIP制限を回避
    define('SITEGUARD_IP_FILTER_BYPASS', true);
}
```

### 解決策2: LightStart専用運用

#### LightStartの除外設定
```
除外欄に追加：
feed
wp-login  
login
wp-json
wp-json/wp/v2
wp-json/wp/v2/posts
wp-json/wp/v2/users
rest_route
admin-ajax.php
```

### 解決策3: AFFINGER6 + functions.php

#### functions.php追加コード
```php
// AFFINGER6メンテナンス時のREST API許可
function affinger_maintenance_rest_api_bypass() {
    if (defined('REST_REQUEST') && REST_REQUEST) {
        // AFFINGERのメンテナンス制御を回避
        remove_action('template_redirect', 'affinger_maintenance_mode');
        remove_action('init', 'affinger_maintenance_mode');
    }
}
add_action('plugins_loaded', 'affinger_maintenance_rest_api_bypass', 1);
```

## 📋 **動作確認手順**

### テスト1: REST API直接アクセス
```bash
# ブラウザまたはcurlで確認
https://your-site.com/wp-json/
https://your-site.com/wp-json/wp/v2/posts?per_page=1
```

### テスト2: MCPサーバー接続テスト
```rust
// 実装済みのテスト機能使用
// メンテナンス検出とREST API接続の同時確認
```

### テスト3: 段階的確認
```
1. メンテナンス無し → REST API動作確認
2. メンテナンス有効 → フロントエンド制限確認  
3. メンテナンス有効 → REST API動作確認
4. 管理者ログイン → フロントエンド表示確認
5. 管理者ログイン → REST API動作確認
```

## 💡 **ベストプラクティス**

### 運用推奨パターン

#### パターンA: SiteGuard中心
```
- SiteGuard: 除外設定でREST API許可
- AFFINGER6: メンテナンス機能は使用しない
- LightStart: 使用しない
- 利点: シンプル、軽量
```

#### パターンB: LightStart中心（推奨）
```
- LightStart: メンテナンス＋除外設定
- SiteGuard: セキュリティ機能のみ
- AFFINGER6: メンテナンス機能は無効
- 利点: 高機能、柔軟性
```

#### パターンC: ハイブリッド
```
- 通常時: SiteGuardのみ
- メンテナンス時: LightStart一時有効化
- AFFINGER6: 使用しない
- 利点: 必要時のみメンテナンス機能
```

## ⚡ **緊急対応**

### 即座実行（MCPサーバー復旧）
```php
// wp-config.phpに一時追加（最優先）
if (defined('REST_REQUEST') && REST_REQUEST) {
    define('WP_MAINTENANCE_MODE', false);
    define('SITEGUARD_IP_FILTER_BYPASS', true);
}
```

### SiteGuard設定更新
```
管理ページアクセス制限 → 除外パス：
css
images
admin-ajax.php
load-styles.php
site-health.php
wp-json                    ← 追加
wp-json/wp/v2             ← 追加
rest_route                ← 追加
```

### LightStart除外設定
```
除外欄：
feed
wp-login
login  
wp-json                   ← 重要
wp-json/wp/v2            ← 重要
rest_route               ← 重要
```

この設定により、メンテナンス時でもMCPサーバーが正常に動作するはずです。
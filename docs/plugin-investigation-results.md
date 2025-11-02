# WP Site Guard と WP Maintenance Mode 除外設定の詳細調査結果

## 🔍 重要な発見

### 1. **WP Site Guard** について

#### 基本情報

-   **完全無料** - 有料版は存在しない
-   開発者：jp-secure（日本の開発者）
-   主な機能：管理画面保護、ログインページ名変更、CAPTCHA

#### ❌ **除外設定の誤解**

```
調査結果：WP Site Guardに有料版は存在しません
- 公式サイト：完全無料プラグイン
- 機能：管理画面IP制限、ログインページ変更等
- 除外設定：基本的な除外機能のみ（有料版なし）
```

#### 実際の機能

```
無料版で利用可能：
✅ Admin Page IP Filter（管理画面IP制限）
✅ Rename Login（ログインページ名変更）
✅ CAPTCHA Protection
✅ Login Lock Feature
❌ 高度な除外設定（このプラグインには存在しない）
```

### 2. **WP Maintenance Mode** について

#### GitHub 情報

-   開発者：andrianvaleanu / Themeisle
-   リポジトリ：https://github.com/andrianvaleanu/WP-Maintenance-Mode
-   **無料版で除外設定利用可能**

#### ✅ **除外設定（無料版で利用可能）**

```php
// GitHub Issue #114 での確認内容
// Exclude設定で以下が可能：

1. URLパス除外
   wp-json
   wp-json/wp/v2
   wp-admin

2. 条件付きバイパス
   bypass=random_string

3. IP除外
   特定IPアドレスの許可

4. ユーザーロール除外
   administrator
   editor
```

## 🛠️ **実際の設定方法**

### WP Maintenance Mode 再インストール時の設定

#### 1. 基本除外設定

```
WordPress管理画面 → 設定 → WP Maintenance Mode
→ General タブ → Exclude 欄に追加：

wp-json
wp-json/wp/v2
wp-json/wp/v2/posts
wp-admin
```

#### 2. ユーザーロール除外

```
Backend Role 設定：
- administrator
- editor（必要に応じて）
```

#### 3. 条件付きバイパス

```
Exclude 欄に追加：
bypass=your_secret_key

アクセス例：
https://your-site.com/?bypass=your_secret_key
```

## 📋 **現状の対策提案**

### 即座に実行可能な対策

#### 1. WP Site Guard 設定確認

```
現在のWP Site Guardで確認：
WordPress管理画面 → SiteGuard → 管理ページアクセス制限
→ 除外設定があるか確認
→ /wp-json/ パスが除外されているか確認
```

#### 2. wp-config.php 追加（推奨）

```php
// wp-config.phpに追加
if (defined('REST_REQUEST') && REST_REQUEST) {
    define('WP_MAINTENANCE_MODE', false);
}
```

#### 3. functions.php 回避（緊急時）

```php
// テーマのfunctions.phpに追加
function bypass_maintenance_for_mcp() {
    if (defined('REST_REQUEST') && REST_REQUEST) {
        remove_action('init', 'wp_maintenance');
        remove_action('wp_loaded', 'wp_maintenance');
        remove_action('get_header', 'wp_maintenance');
    }
}
add_action('plugins_loaded', 'bypass_maintenance_for_mcp', 1);
```

### 長期的な解決策

#### 1. WP Maintenance Mode 再インストール

```
推奨理由：
✅ 無料で高度な除外設定が可能
✅ GitHubで活発に開発継続
✅ REST API個別制御が可能
✅ ユーザーロール別制御が可能
```

#### 2. 代替プラグイン検討

```
推奨代替案：
1. Coming Soon Page & Maintenance Mode（明示的REST API設定あり）
2. Maintenance Mode（WebFactory製、API制御機能付き）
3. Under Construction（REST API除外機能付き）
```

## 🚨 **重要な結論**

### 誤解の訂正

1. **WP Site Guard に有料版は存在しない**
2. **WP Maintenance Mode は無料版で除外設定可能**
3. **除外設定だけでは不十分な場合が多い**

### 推奨アクション

1. **即座**：wp-config.php 修正で REST API 許可
2. **短期**：WP Maintenance Mode 再インストール＋適切設定
3. **長期**：MCP サーバー側メンテナンス検出機能活用

### コスト面

```
全ての対策が無料で実行可能：
- WP Site Guard：完全無料
- WP Maintenance Mode：無料版で十分
- wp-config.php修正：無料
- functions.php修正：無料
```

## 📞 **次のステップ**

1. **WP Site Guard 設定確認**（現在インストール済み）
2. **wp-config.php 一時修正**（即効性）
3. **WP Maintenance Mode 再検討**（根本解決）
4. **MCP サーバー接続テスト**（動作確認）

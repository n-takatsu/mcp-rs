# WordPress セキュリティプラグイン対応状況

## 🛡️ 主要セキュリティプラグインとの互換性

### ✅ 完全対応 (REST API 影響なし)

-   **WP Site Guard** - ログインページ保護のみ
-   **Wordfence** - REST API 設定で制限しない限り対応
-   **iThemes Security** - デフォルト設定で対応
-   **Sucuri Security** - REST API 制限なしで対応
-   **All In One WP Security** - 基本設定で対応

### ⚠️ 注意が必要 (設定次第)

-   **Wordfence** - "Block unauthorized access to REST API" 有効時は注意
-   **Jetpack Protect** - REST API 制限設定時は注意

### 🔧 問題が発生する可能性がある設定

#### Wordfence の場合:

```
Wordfence → Firewall → Brute Force Protection
→ "Prevent unauthorized access to REST API"
→ この設定を無効にする
```

#### iThemes Security の場合:

```
iThemes Security → WordPress Tweaks
→ "Disable XML-RPC" は問題なし (REST APIは別)
→ "REST API Access" 設定を確認
```

## 🧪 接続テストコマンド

### curl でのテスト:

```bash
# REST API root の確認
curl -i "https://your-wordpress-site.com/wp-json/wp/v2/"

# Application Password での認証テスト
curl -u "username:app_password" \
  "https://your-wordpress-site.com/wp-json/wp/v2/posts"
```

### mcp-rs での接続テスト:

```bash
# 将来実装予定
./mcp-rs --test-wordpress-connection
```

## 🚨 トラブルシューティング

### 403 Forbidden エラーの場合:

1. セキュリティプラグインの REST API 制限を確認
2. Application Password が正しく設定されているか確認
3. HTTPS を使用しているか確認

### 404 Not Found エラーの場合:

1. WordPress のパーマリンク設定を確認
2. `.htaccess` ファイルの権限を確認
3. REST API が有効になっているか確認

### タイムアウトエラーの場合:

1. WordPress サイトのパフォーマンスを確認
2. セキュリティプラグインの DDoS 保護設定を確認
3. mcp-rs の timeout 設定を調整

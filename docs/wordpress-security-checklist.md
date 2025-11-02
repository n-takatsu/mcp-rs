# WordPress Security Plugin 対応チェックリスト

## ✅ 必須確認事項

### 1. Application Password 設定

```json
管理画面で「ユーザー」→「プロフィール」→「アプリケーションパスワード」
新しいパスワード名: "MCP Server"
生成されたパスワード: xxxx xxxx xxxx xxxx xxxx xxxx
```

### 2. REST API 動作確認

```powershell
# Basic認証テスト（PowerShell）
$auth = [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("username:xxxx xxxx xxxx xxxx xxxx xxxx"))
$headers = @{ "Authorization" = "Basic $auth" }
Invoke-RestMethod -Uri "https://your-site.com/wp-json/wp/v2/posts?per_page=1" -Headers $headers
```

### 3. WP Site Guard 設定の影響確認

#### ✅ 影響しない（MCP 動作に問題なし）

-   ログインページ URL 変更（wp-admin → wp-login123 等）
-   管理画面への IP 制限
-   ログイン試行回数制限
-   画像認証/2 段階認証

#### ⚠️ 影響する可能性（要設定確認）

-   REST API 完全無効化
-   外部からの API 呼び出し拒否
-   User-Agent 制限

### 4. セキュリティプラグイン別対応

#### WP Site Guard

```
設定確認箇所：
- 「ログイン」→「XMLRPC無効化」（有効でOK）
- 「管理ページアクセス制限」→「REST API」（無効にする必要なし）
```

#### Wordfence

```
設定確認箇所：
- 「Login Security」→「XML-RPC Access」（無効でOK）
- 「Firewall」→「Block fake Google crawlers」（MCP影響なし）
```

#### All In One WP Security

```
設定確認箇所：
- 「Firewall」→「Basic Firewall Rules」
- 「Brute Force」→「Login Lockout」（Application Password影響なし）
```

## 🔧 トラブルシューティング

### 接続テストコマンド

```rust
// WordPressプラグインに追加済み
async fn test_connection(&self) -> Result<String, Box<dyn std::error::Error>> {
    let response = self.client
        .get(&format!("{}/wp-json/wp/v2/posts", self.config.url))
        .query(&[("per_page", "1")])
        .send()
        .await?;

    if response.status().is_success() {
        Ok("✅ WordPress REST API接続成功".to_string())
    } else {
        Err(format!("❌ 接続失敗: {}", response.status()).into())
    }
}
```

### よくあるエラーと対処法

#### 401 Unauthorized

```
原因: Application Passwordが間違っている
対処: 新しいApplication Passwordを生成し直す
```

#### 403 Forbidden

```
原因: セキュリティプラグインがREST APIを制限
対処: プラグイン設定でREST API許可を確認
```

#### 404 Not Found

```
原因: WordPressのパーマリンク設定問題
対処: 「設定」→「パーマリンク」で保存し直す
```

## 📋 運用推奨事項

1. **定期的な接続テスト**

    - MCP サーバー起動時の自動テスト実装済み

2. **Application Password の定期更新**

    - 3-6 ヶ月毎の更新推奨

3. **セキュリティプラグイン更新時の確認**

    - プラグイン更新後は接続テスト実行

4. **ログ監視**
    - WordPress 側で API 使用ログ確認
    - MCP 側でエラーログ監視

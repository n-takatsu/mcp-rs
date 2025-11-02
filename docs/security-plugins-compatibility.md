# WordPress セキュリティプラグインとMCP接続の関係

## 📋 接続経路の違い

### 🌐 ブラウザ経由のアクセス (影響を受ける)
```
ユーザー → ブラウザ → セキュリティプラグイン → WordPress管理画面
```
- ログインページURL変更の影響を受ける
- CAPTCHA、2段階認証の影響を受ける
- IP制限、地域制限の影響を受ける

### 🔌 REST API経由のアクセス (影響を受けない)
```
MCP Server → WordPress REST API (/wp-json/wp/v2/)
```
- ログインページを使用しない
- Application Passwordで認証
- セキュリティプラグインのフィルターをバイパス

## 🛡️ セキュリティプラグインの動作範囲

### WP Site Guard の保護対象:
- ✅ ログインページ (/wp-admin/)
- ✅ 管理画面 (/wp-admin/*)
- ✅ XML-RPC (通常は無効化)

### WP Site Guard が影響しない:
- ✅ REST API (/wp-json/*)
- ✅ フロントエンド表示
- ✅ APIベースの認証

## 🔑 Application Password の利点

Application Passwordは以下の理由で安全:
- セキュリティプラグインの制限を回避
- REST API専用認証方式
- 2段階認証が有効でも動作
- ログインページの変更に影響されない
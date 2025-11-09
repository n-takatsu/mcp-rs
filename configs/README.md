# MCP-RS Configuration Files v0.15.1

MCP-RSの設定ファイル管理ディレクトリです。用途別に整理された設定ファイルを提供します。

## 🆕 v0.15.1 統合アップデート
- ルートレベルの分散設定ファイル（mcp-config*.toml）を `configs/` に統合
- Transport統合アーキテクチャ対応
- 必須ハンドラー設定の追加
- 重複設定の解消

## 📁 ディレクトリ構成

### 🚀 Production (`production/`)
本番環境で使用する設定ファイル

- **`main.toml`** - メイン設定（デフォルトSTDIO）
  - Transport統合アーキテクチャ対応
  - ファイルシステムハンドラー（読み取り専用）
  - 標準的な本番環境設定

- **`claude-desktop.toml`** - Claude Desktop統合用設定
  - STDIO通信専用
  - ログファイル出力のみ（コンソール出力制限）
  - 本番レベルのセキュリティ設定

- **`web-ui.toml`** - Web UI用設定  
  - HTTP Transport使用
  - コンソール + ファイルログ出力
  - CORS設定有効
  - WordPress + ファイルシステムハンドラー

### 🔧 Development (`development/`)
開発・テスト用設定ファイル

- **`demo.toml`** - デモンストレーション用
- **`http-transport.toml`** - HTTP Transport テスト用
- **`tcp.toml`** - TCP通信テスト用  
- **`testing.toml`** - 単体テスト用設定

### 📚 Examples (`examples/`)
設定例・学習用ファイル

- **`multi-handler.toml`** - 複数ハンドラー設定例
- **`log-policy-demo.toml`** - ログポリシー設定デモ
- **`module-separated.toml`** - モジュール分離ログ例

### 📝 Templates (`templates/`)
設定テンプレートファイル

- **`basic.toml`** - 基本設定テンプレート
- **`advanced.toml`** - 高度な設定テンプレート

## 🚀 使用方法

### Claude Desktop統合
```bash
mcp-rs --config configs/production/claude-desktop.toml
```

### Web UI開発
```bash  
mcp-rs --config configs/production/web-ui.toml
```

### ローカル開発
```bash
mcp-rs --config configs/development/demo.toml
```

### カスタム設定作成
```bash
# テンプレートをコピーして編集
cp configs/templates/basic.toml my-config.toml
# 編集後
mcp-rs --config my-config.toml
```

## 📋 設定ファイル選択ガイド

| 用途 | 設定ファイル | Transport | ログ出力 |
|------|--------------|-----------|----------|
| Claude Desktop使用 | `production/claude-desktop.toml` | STDIO | ファイルのみ |
| Web UI使用 | `production/web-ui.toml` | HTTP | コンソール + ファイル |  
| API開発・テスト | `development/http-transport.toml` | HTTP | 詳細ログ |
| デモ・プレゼン | `development/demo.toml` | HTTP | 標準ログ |

## ⚠️ 重要な注意事項

### Claude Desktop使用時
- **必ず** `production/claude-desktop.toml` を使用
- STDIO通信では標準出力にログが混在すると動作不能
- `log_level = "error"` でコンソール出力を最小限に

### セキュリティ
- 本番環境では環境変数 `${WORDPRESS_PASSWORD}` を使用
- パスワードを平文で設定ファイルに記載しない
- `.env` ファイルで機密情報を管理

### ログ管理
- `separation = "separated"` でモジュール別ログファイル分離推奨
- ログ保持ポリシーを適切に設定
- ディスク容量を定期的に監視

## 🔗 関連ドキュメント

- [Claude Desktop統合ガイド](../project-docs/CLAUDE_DESKTOP_INTEGRATION.md)
- [アーキテクチャガイド](../project-docs/architecture.md)
- [セキュリティガイド](../project-docs/security-guide.md)
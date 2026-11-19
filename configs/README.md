# MCP-RS Configuration Files

MCP-RSの設定ファイルの例です。用途別に置いています。

- mcp-rsが読むのは、`[server]`・`[transport]`・`[handlers]`（`[handlers.wordpress]`）・`[database]`・`[[plugins]]`・`[audit]`です
- 知らないキーはエラーにならず、黙って無視されます。そのため、ここの例は、どれも読まれないキーが無いことを、テスト（`tests/config_examples_test.rs`）で確かめています（#557）
- v0.23.0までは、`production/`等の例に、mcp-rsが読まないキーがありました。例えば、存在しない「ファイルシステムハンドラー」（`[filesystem]`）・`[security]`・`[logging]`です
- ファイルを読みたいときは、MCPのファイルシステムのサーバーを、プラグイン（`[[plugins]]`）として足します

## 📁 ディレクトリ構成

## 🚀 Production (`production/`)

本番環境で使用する設定ファイル

- **`main.toml`** - いちばん小さいSTDIOの設定
  - ハンドラーは無し（`mcp_rs_status`だけ）。WordPressの例をコメントで置いています

- **`claude-desktop.toml`** - Claude DesktopとClaude Code用（STDIO）
  - WordPressの資格情報を、環境変数`WORDPRESS_URL`・`WORDPRESS_USERNAME`・`WORDPRESS_PASSWORD`から読みます

- **`web-ui.toml`** - このPCのWebクライアント用（HTTP、`127.0.0.1:8081`）
  - CORS有効、WordPress

## 🔧 Development (`development/`)

開発・テスト用設定ファイル

- **`demo.toml`** - デモンストレーション用
- **`http-transport.toml`** - HTTP Transport テスト用（`/ws`も有効）
- **`testing.toml`** - テスト用（STDIO、WordPressを環境変数から）
- `tcp.toml`は削除しました（#557）。mcp-rsにTCPのトランスポートはありません

## 📚 Examples (`examples/`)

設定例・学習用ファイル

- **`multi-handler.toml`** - WordPress・PostgreSQL（`[database]`）・プラグイン（`[[plugins]]`）を一緒に使う例
  - v0.23.0までは、mcp-rsが読まない形式（`[handlers.<名前>]`に`type = "..."`）の例で、起動もしなかった（#557）
- **`multi-site.toml`** - 1つのmcp-rsで、2つのWordPressのサイトを使う例（`[handlers.wordpress.sites.<名前>]`、v0.30.0以降）
- **`claude-desktop-config.example.json`**・**`copilot-studio.example.json`** - MCPのクライアントへの登録の例（`--config`で設定ファイルを渡す。`mcp-rs init`が書く形）
- **`log-policy-demo.toml`** - ログポリシー設定デモ
- **`module-separated.toml`** - モジュール分離ログ例

## 📝 Templates (`templates/`)

設定テンプレートファイル

- **`basic.toml`** - 基本設定テンプレート
- **`advanced.toml`** - 高度な設定テンプレート

## 🚀 使用方法

## Claude Desktop統合

```bash
mcp-rs --config configs/production/claude-desktop.toml
```

## Web UI開発

```bash
mcp-rs --config configs/production/web-ui.toml
```

## ローカル開発

```bash
mcp-rs --config configs/development/demo.toml
```

## カスタム設定作成

```bash

## テンプレートをコピーして編集

cp configs/templates/basic.toml my-config.toml

## 編集後

mcp-rs --config my-config.toml
```

## 📋 設定ファイル選択ガイド

| 用途 | 設定ファイル | Transport | ログ出力 |
|------|--------------|-----------|----------|
| Claude Desktop使用 | `production/claude-desktop.toml` | STDIO | ファイル + 標準エラー |
| Web UI使用 | `production/web-ui.toml` | HTTP | ファイル + 標準エラー |
| API開発・テスト | `development/http-transport.toml` | HTTP | 詳細ログ |
| デモ・プレゼン | `development/demo.toml` | HTTP | 標準ログ |

## ⚠️ 重要な注意事項

## Claude Desktop使用時

- `production/claude-desktop.toml`から始めると簡単です
- STDIOでは、標準出力はJSON-RPCの通信だけに使います。mcp-rsは、ログを標準出力に書きません（ファイルと標準エラー）

## セキュリティ

- 本番環境では環境変数 `${WORDPRESS_PASSWORD}` を使用
- パスワードを平文で設定ファイルに記載しない
- Claude Desktopでは、`claude_desktop_config.json`の`env`で渡すか、OSの資格情報ストア（`keyring`）を使います

## ログ管理

- `separation = "separated"` でモジュール別ログファイル分離推奨
- ログ保持ポリシーを適切に設定
- ディスク容量を定期的に監視

## 🔗 関連ドキュメント

- [Claude Desktop統合ガイド](../project-docs/CLAUDE_DESKTOP_INTEGRATION.md)
- [Architecture](https://n-takatsu.github.io/mcp-rs/docs/architecture/)
- [Security](https://n-takatsu.github.io/mcp-rs/docs/security/)

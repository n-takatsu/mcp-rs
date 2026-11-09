# mcp-rs

🚀 **本番対応** Model Context Protocol (MCP) の Rust 実装。WordPress とそれ以外のシステムとの AI エージェント統合を実現します。

> **[日本語](README.ja.md)** | **[English](README.md)**

[![Version](https://img.shields.io/badge/Version-v0.20.0-blue)](https://github.com/n-takatsu/mcp-rs/releases/tag/v0.20.0)
![Architecture](https://img.shields.io/badge/Architecture-Production--Ready-green)
![Implementation](https://img.shields.io/badge/WordPress_Tools-32_Available-green)
![License](https://img.shields.io/badge/License-MIT%2FApache--2.0-green)

## 概要

`mcp-rs` は、**包括的で実戦テスト済み**の MCP（Model Context Protocol）の Rust 実装で、**完全な WordPress 統合**を提供します。レイヤード・アーキテクチャで構築され、AI エージェントが標準化された JSON-RPC インターフェースを通じて高度なコンテンツ管理を実行できるようにします。このフレームワークは 32 の WordPress ツール、エンタープライズレベルのセキュリティ、強力な型安全性を備え、GitHub Copilot や他の AI コーディングアシスタントでの本番利用に最適化されています。

## 🎯 対象ユーザー

## **AI 開発者** 🤖

Claude Desktop アプリ、GPT 統合、カスタム AI エージェントを構築していますか？ mcp-rs は包括的な WordPress ツールを備えた本番対応の Model Context Protocol 実装を提供します。

## **エンタープライズ WordPress チーム** 🏢

大規模な WordPress デプロイメントを管理していますか？エンタープライズグレードのセキュリティ、自動化されたコンテンツ管理、シームレスな CI/CD 統合を取得できます。

## **DevOps エンジニア** ⚙️

WordPress 運用を自動化していますか？包括的なヘルスチェック、監視、本番対応のエラーハンドリングを備えた 32 の実戦テスト済みツールを利用できます。

## **Rust 愛好者** 🦀

高品質な Rust コードベースに貢献したいですか？クリーンなアーキテクチャと包括的なドキュメントを備えた、205+ テスト、警告ゼロのプロジェクトにご参加ください。

## **セキュリティチーム** 🔒

mcp-rsをHTTPで公開しますか？APIキーやOAuth 2.1のトークンで認証し、呼び出し元ごとに使えるツールを絞り、すべてのツールの呼び出しを監査ログに残せます。

## 🚀 主要機能

## **コア機能**

- **JSON-RPC 2.0 サーバー**: `axum` を使用したフル機能 JSON-RPC サーバー実装
- **マルチトランスポート対応**: STDIO、HTTP、WebSocket 通信プロトコル
- **プラグインアーキテクチャ**: `McpHandler` トレイトによる拡張可能なハンドラーベースシステム
- **型安全な設定**: 環境変数オーバーライドを備えた TOML ベース設定
- **本番対応エラーハンドリング**: 構造化ログを備えた包括的なエラータイプ
- **Async/Await**: 高性能非同期操作のための `tokio` ベース

## **WordPress 統合（32 ツール）**

- **投稿・固定ページ管理**: 作成・取得（編集用の原文を全文）・更新・削除。SEO メタデータと予約投稿に対応
- **メディア管理**: base64 でのアップロード（形式・拡張子・内容を検証）、一覧、メタデータ更新、削除、アイキャッチ画像の設定
- **カテゴリ・タグ管理**: 作成・更新・削除。カテゴリの階層に対応
- **コメント**: コメントの取得
- **YouTube・ソーシャル埋め込み**: セキュリティ検証付きリッチメディア統合
- **サイト設定**: サイト設定とフロントページ設定の取得・更新
- **ヘルスチェック**: 接続・認証・権限・メディアアップロード可否の確認

### ライブデモサイト

`examples/wordpress_site_builder_demo.rs` の例は、公開 WordPress デモサイト [redring.jp](https://redring.jp/) を構築・更新します。mcp-rs がサイトを立ち上げ、コンテンツを整理して実運用する流れを示す実例です。

## **セキュリティ**

- **認証情報の暗号化**: WordPressのApplication Passwordを、AES-GCM-256（PBKDF2で鍵を導出）で暗号化して保存できる。環境変数から読むこともできる
- **HTTPモードの認証**: APIキー、または外部の認可サーバーが発行したOAuth 2.1のアクセストークン（JWT）
- **呼び出し元ごとのツールの制限**: APIキー・トークンごとに、使えるツールを絞れる（例: `@read_only`）
- **TLS・mTLS**: 最低のTLSバージョンを指定したHTTPS、クライアント証明書（任意）
- **ネットワークポリシー・リプレイ攻撃の防止・侵入検知**（HTTP・WebSocketの待ち受け）
- **レート制限**（ツールの呼び出し）
- **監査ログ**: 誰がどのツールを呼び、結果がどうだったか

## **技術的優秀性**

- **非同期アーキテクチャ**: 高性能並行処理のための Tokio ベース
- **型安全性**: 100% メモリ安全な Rust 実装
- **包括的テスト**: 100% 合格率の 205+ テスト
- **警告ゼロ**: `Clippy`（Rustリンター）警告ゼロのクリーンなコードベース <!-- cSpell:ignore Clippy -->
- **本番対応**: 最適化されたビルドプロファイルとエラーハンドリング

## 📊 品質指標

| 指標 | 値 |
|------|-----|
| **総テスト数** | 205+ (100% 合格) |
| **コードカバレッジ** | 包括的 |
| **パフォーマンス** | 本番最適化 |
| **ドキュメント** | 完全な API ドキュメント |

## 🚀 クイックスタート

## 前提条件

- Rust 1.70+ (2021 edition)
- アプリケーションパスワードが有効な WordPress サイト
- WordPress REST API へのネットワークアクセス

## インストール

### 🚨 Claude Desktop ユーザーへの重要なお知らせ

**Claude Desktop は STDIO（標準入出力）通信を使用します。ログメッセージが標準出力に混在すると通信が破綻するため、必ず専用設定をご使用ください。**

```bash

## Claude Desktop 用（重要：専用設定を使用）

cargo run -- --config configs/production/claude-desktop.toml

## Web UI 用（HTTP アクセス）

cargo run -- --config configs/development/http-transport.toml
```

> 📖 詳細な設定については [Claude Desktop 統合ガイド](./project-docs/CLAUDE_DESKTOP_INTEGRATION.md) をご覧ください。

### Claude Code

Claude Code も STDIO で mcp-rs と通信し、同じ設定ファイルを使います。mcp-rs をビルドしてから、絶対パスで登録します。`--` より後ろが mcp-rs の起動コマンドです。

```bash
cargo build --release

claude mcp add mcp-rs --scope user \
  -e WORDPRESS_URL=https://your-wordpress-site.com \
  -e WORDPRESS_USERNAME=your_username \
  -e WORDPRESS_PASSWORD="your application password" \
  -- /path/to/mcp-rs/target/release/mcp-rs --config /path/to/mcp-rs/configs/production/claude-desktop.toml
```

接続は `claude mcp list`、または Claude Code 内の `/mcp` で確認できます。`-e` で渡した値は Claude Code の設定に保存されるため、実際の認証情報で `--scope project`（共有される `.mcp.json` に書き込まれる）を使わないでください。

> 📖 `.mcp.json` での登録、認証情報の暗号化、トラブルシューティングは [Claude Code 統合ガイド](./project-docs/CLAUDE_CODE_INTEGRATION.md) をご覧ください。

### セットアップ

```bash
git clone https://github.com/n-takatsu/mcp-rs.git
cd mcp-rs
cargo build --release
```

ビルドしたら、設定ファイルを指定して起動します。すぐに使える設定ファイルが`configs/production/`にあります。

- `claude-desktop.toml`: STDIO用（Claude Desktop・Claude Code）。WordPressの接続情報は、環境変数`WORDPRESS_URL`・`WORDPRESS_USERNAME`・`WORDPRESS_PASSWORD`から読み込みます
- `web-ui.toml`: HTTP用（Webクライアント）

```bash
./target/release/mcp-rs --config configs/production/claude-desktop.toml
```

`mcp-rs --version`でインストールされているバージョンを、`mcp-rs --help`で使えるオプションを表示します（v0.19.1以降）。知らないオプションや、パスの無い`--config`があると起動しません（v0.19.2以降。v0.19.1はログに警告を出すだけでした）。起動しなくなった場合は、MCPのクライアントの設定にあるmcp-rsの`args`を確かめてください。

MCPのクライアントから（たとえばClaudeに頼んで）`mcp_rs_status`ツールを呼ぶと、バージョン、使っている設定ファイル、ログと監査ログの書き込み先、各プラグインの状態を確かめられます（v0.19.2以降）。

設定ファイルを自分で書く場合は、次の最小構成から始めてください。

## 基本設定

`mcp-config.toml` を作成:

```toml
[transport]
type = "stdio"          # HTTPモードは "http"（[transport.http] を参照）

[server]
log_level = "warn"      # STDIOモードでは、ログは標準出力ではなくファイルに出力されます

[handlers.wordpress]
enabled = true
url = "https://your-wordpress-site.com"
username = "your-username"
password = "your-application-password"   # WordPress アプリケーションパスワード
```

`./target/release/mcp-rs --config mcp-config.toml`で起動します。パスワードを設定ファイルに書かない場合は、`${WORDPRESS_PASSWORD}`（環境変数）か[認証情報の暗号化](https://n-takatsu.github.io/mcp-rs/docs/security/#encrypted-wordpress-credentials)を使ってください。`${...}`の環境変数が設定されていないと、mcp-rsは起動しません。

### HTTP TLS/mTLS 設定（任意）

HTTP トランスポートで TLS とクライアント証明書認証（mTLS）を使う場合は、次のように設定します。

```toml
[transport]
transport_type = "http"

[transport.http]
addr = "127.0.0.1"
port = 8080
tls_enabled = true
tls_cert_path = "./certs/server.crt"
tls_key_path = "./certs/server.key"
mtls_enabled = true
mtls_ca_cert_path = "./certs/ca.crt"
enforce_https = true
min_tls_version = "1.3"
hsts_enabled = true
```

注意:

- `mtls_enabled = true` の場合、`tls_enabled = true` が必須です
- `mtls_enabled = true` の場合、`mtls_ca_cert_path` が必須です
- 設定した CA で検証できないクライアント証明書、または証明書未提示のクライアントは TLS ハンドシェイクで拒否されます

### ログファイル

ログは、次のフォルダのうち、書き込める最初のフォルダに出力されます。

1. `mcp-rs`の実行ファイルと同じフォルダの`logs`（zipを展開した場所など）
2. ユーザーのフォルダ: Windowsは`%USERPROFILE%\.mcp-rs\logs`、それ以外は`$XDG_STATE_HOME/mcp-rs/logs`か`~/.local/state/mcp-rs/logs`。`C:\Program Files\MCP-rs`にインストールした場合は、常にここになります
3. カレントフォルダの`logs`、次に一時フォルダの`mcp-rs/logs`

使っているログファイルは、`mcp_rs_status`ツールで確認できます。`fallback`と表示されるのは、1にも2にも書き込めなかった場合だけです。

ファイル名の末尾は`mcp-rs.log.2026-11-09`のような日付で、1日ごとに新しいファイルになります。ファイル名の日付はUTCなので、日本時間では毎朝9:00に新しいファイルになります。ログの中の時刻は、ローカル時刻です。

## 🏗️ アーキテクチャ

```text
┌─────────────────────┐    ┌─────────────────────┐    ┌─────────────────────┐
│   AI エージェント    │───▶│    MCP サーバー     │───▶│   WordPress サイト   │
│  (Claude Desktop,   │    │   (mcp-rs)          │    │   (REST API)        │
│   カスタム AI など)  │    │                     │    │                     │
└─────────────────────┘    └─────────────────────┘    └─────────────────────┘
                                      │
                                      ▼
                           ┌─────────────────────┐
                           │   セキュリティ層    │
                           │ • 認証              │
                           │ • ツールの制限      │
                           │ • レート制限        │
                           │ • 監査ログ          │
                           └─────────────────────┘
```

## 📚 ドキュメント

- [🚀 クイックスタートガイド](./project-docs/quick-start.md)
- [🔧 設定リファレンス](./project-docs/configuration.md)
- [🔒 セキュリティガイド](./project-docs/security.md)
- [🏗️ アーキテクチャ概要](./project-docs/architecture.md)
- [📝 API リファレンス](./project-docs/api-reference.md)
- [🔄 Claude Desktop 統合](./project-docs/CLAUDE_DESKTOP_INTEGRATION.md)
- [🔄 Claude Code 統合](./project-docs/CLAUDE_CODE_INTEGRATION.md)

## 🛠️ 開発

## ソースからのビルド

```bash

## 開発ビルド

cargo build

## 最適化された本番ビルド

cargo build --release

## テスト実行

cargo test

## 特定の設定で実行

cargo run -- --config configs/development/http-transport.toml
```

## 使用例

プロジェクトには様々な機能を示す包括的な例が含まれています：

- **コア例**: `examples/` ディレクトリに格納
- **データベース例**: `examples.disabled/` ディレクトリに格納
  - `mysql_engine_test.rs`: MySQL データベースエンジンのテスト（`database` フィーチャーが必要）
  - 使用方法: `cargo run --example mysql_engine_test --features database,mysql-backend`

> **注意**: データベース依存の例は、`database` フィーチャーがデフォルトで有効でない場合のCI安定性を確保するため、`examples.disabled/` に移動されています。

## テスト

```bash

## 全テスト実行

cargo test

## 出力付きで実行

cargo test -- --`nocapture`  # テスト出力を表示するオプション <!-- cSpell:ignore nocapture -->

## 特定のテストモジュール実行

cargo test wordpress_api
```

## 🤝 コントリビューション

コントリビューションを歓迎します！ガイドラインについては [CONTRIBUTING.md](CONTRIBUTING.md) をご覧ください。

1. リポジトリをフォーク
2. フィーチャーブランチを作成 (`git checkout -b feature/amazing-feature`)
3. 変更をコミット (`git commit -m 'Add some amazing feature'`)
4. ブランチにプッシュ (`git push origin feature/amazing-feature`)
5. プルリクエストを開く

## 📄 ライセンス

このプロジェクトはデュアルライセンスです：

- [MIT ライセンス](LICENSE-MIT)
- [Apache License 2.0](LICENSE-APACHE)

お好みのライセンスを選択できます。

## 🙏 謝辞

- Anthropic による [Model Context Protocol](https://modelcontextprotocol.io/)
- [WordPress REST API](https://developer.wordpress.org/rest-api/)
- 優秀なツールとライブラリを提供する Rust コミュニティ

## 📞 サポート

- 📖 [ドキュメント](./docs/)
- 🐛 [イシュートラッカー](https://github.com/n-takatsu/mcp-rs/issues)
- 💬 [ディスカッション](https://github.com/n-takatsu/mcp-rs/discussions)

---

❤️ と Rust 🦀 で構築

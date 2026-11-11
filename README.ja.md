# mcp-rs

AIのクライアントに、WordPress・PostgreSQL・外部のMCPプラグインのツールを提供する、Model Context Protocol（MCP）のサーバーのRust実装です。

> **[日本語](README.ja.md)** | **[English](README.md)**

[![Version](https://img.shields.io/badge/Version-v0.22.0-blue)](https://github.com/n-takatsu/mcp-rs/releases/tag/v0.22.0)
![Implementation](https://img.shields.io/badge/WordPress_Tools-32_Available-green)
![License](https://img.shields.io/badge/License-MIT%2FApache--2.0-green)

## 概要

`mcp-rs`はRustで書かれたMCPのサーバーです。Claude Desktop・Claude Code等のMCPのクライアントはSTDIOで、それ以外のクライアントはHTTPでつなぎます。次の機能があります。

- WordPress REST APIを使う32のツール（投稿・固定ページ・メディア・カテゴリ・タグ・コメント・サイト設定）
- 1つのPostgreSQLのデータベースを読むSQLのツール（書き込みは許したときだけ）
- プラグインとして起動した外部のMCPサーバーのツール（子プロセス・Dockerのコンテナ・KubernetesのJob）
- HTTPモード向け: APIキー・OAuth 2.1の認証、呼び出し元ごとのツールの制限、TLS・mTLS、レート制限、監査ログ

## 🚀 主要機能

## **コア機能**

- **JSON-RPC 2.0のMCP**: プロトコルの版は2025-11-25まで
- **トランスポート**: STDIOと、HTTP（`/ws`のWebSocketも任意で）
- **プラグイン**: STDIOで話す任意のMCPサーバーのツールを加える。子プロセス・Dockerのコンテナ・KubernetesのJobで起動する。[Plugins](https://n-takatsu.github.io/mcp-rs/docs/plugins/)を参照
- **設定**: 1つのTOMLファイル。`${VAR}`で環境変数の値を使える
- **状態のツール**: `mcp_rs_status`で、バージョン・設定ファイル・ログのファイル・起動しなかったものが分かる

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

## **データベース連携（PostgreSQL）**

- **設定は1つの節**: 設定ファイルに`[database]`を書く（`engine = "postgresql"`、ホスト・データベース・ユーザー、`password = "${VAR}"`）
- **既定は読み取りだけ**: `execute_query`（1つのSELECT。読み取り専用のトランザクションで実行してロールバック）と`get_schema`
- **書き込みは許したときだけ**: `allow_write = true`のときだけ`execute_command`（1つのINSERT・UPDATE・DELETE）を出す
- **型の付いたパラメーター**: `$1`・`$2`…はバインドし、SQLの文字列には埋め込まない。文字列は列の型に変換する
- **上限**: 既定で、結果は1000行、1つのSQLは30秒、接続は5つまで
- **監査ログ**: 呼び出しをSQLとともに記録する（パラメーターの値は記録しない）

読み取り専用のロールで接続してください。詳しくは[Database Integration Guide](https://n-takatsu.github.io/mcp-rs/docs/database/)を参照してください。

## 🚀 クイックスタート

## 前提条件

- Rust 1.70+ (2021 edition)
- アプリケーションパスワードが有効な WordPress サイト
- WordPress REST API へのネットワークアクセス

## インストール

### Claude Desktop

Claude Desktopはmcp-rsを起動し、STDIOで通信します。STDIOモードでは、標準出力にはMCPのメッセージだけを書き、ログはファイル（と標準エラー）に書きます。`claude_desktop_config.json`に絶対パスで登録し、認証情報は`env`で渡します。

```json
{
  "mcpServers": {
    "mcp-rs": {
      "command": "/path/to/mcp-rs/target/release/mcp-rs",
      "args": ["--config", "/path/to/mcp-config.toml"],
      "env": { "WORDPRESS_PASSWORD": "your application password" }
    }
  }
}
```

> 📖 設定とトラブルシューティングは [Claude Desktop 統合ガイド](./project-docs/CLAUDE_DESKTOP_INTEGRATION.md) をご覧ください。

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

`./target/release/mcp-rs --config mcp-config.toml`で起動します。パスワードを設定ファイルに書かない場合は、`${WORDPRESS_PASSWORD}`（環境変数）か[認証情報の暗号化](https://n-takatsu.github.io/mcp-rs/docs/security/#encrypted-wordpress-credentials)を使ってください。`${...}`の環境変数が設定されていないと、mcp-rsは起動しません（`[database]`の場合は、データベースのツールだけを出さずに起動します）。

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

ファイル名の末尾は`mcp-rs.log.2026-11-09`のような日付で、ローカル時刻の0時に新しいファイルになります。ファイル名の日付も、ログの中の時刻も、ローカル時刻です（以前のバージョンでは、ファイル名の日付はUTCでした）。動き続けているmcp-rsが次の日のファイルに切り替えると、新しいファイルの先頭に`=== mcp-rs … continued at … / pid … / instance … / log: … ===`のような行が入るので、どのファイルも単独で、どのプロセスが書いたかが分かります。2つのプロセスが動いている場合（下記）は、プロセスごとに1行ずつ、`instance`の違う行が入ります。

Claude Desktopは、`claude_desktop_config.json`に1つだけ登録したmcp-rsを、2つのプロセスとして起動することがあります。Claude Desktopの動作で、mcp-rsの設定の問題ではありません。2つのプロセスが同じログファイルに書くので、ログが二重になったように見えます。

- ログの各行にはプロセスID（`[pid …]`）が付いているので、2つのプロセスを区別できます
- 起動時の見出しの行（`=== mcp-rs … started at … / pid … / instance … / log: … ===`）も、プロセスごとに出ます。Windowsはプロセスの番号を使い回すので、再起動の後は、同じ`pid`が別のプロセスのことがあります。`[pid …]`は、その上にある直近の見出しの行と合わせて見てください。見出しの行には`instance`（プロセスの番号と起動時刻）も出ます。監査ログはツールの呼び出しごとに同じ`instance`を記録するので、2つのログを突き合わせられます
- WordPressの接続の確認など、起動時の記録はプロセスごとに出ます。2回ずつ出るのは正常です

監査ログ（誰がどのツールを呼んだか）は、ログと並べて、Windowsでは`%USERPROFILE%\.mcp-rs\audit\audit.log`（それ以外は`$XDG_STATE_HOME/mcp-rs/audit`か`~/.local/state/mcp-rs/audit`）に書きます。`[audit] file_path`で変えられます。バージョン0.21.0までは、設定ファイルと同じフォルダの`audit.log`でした。そのファイルは移さずに残します。詳しくは[セキュリティ](https://n-takatsu.github.io/mcp-rs/docs/security/#audit-log)を見てください。

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

ドキュメントのサイトは[n-takatsu.github.io/mcp-rs](https://n-takatsu.github.io/mcp-rs/docs/)です（英語）。

- [WordPress](https://n-takatsu.github.io/mcp-rs/docs/wordpress/)
- [データベース（PostgreSQL）](https://n-takatsu.github.io/mcp-rs/docs/database/)
- [プラグイン](https://n-takatsu.github.io/mcp-rs/docs/plugins/)
- [セキュリティ](https://n-takatsu.github.io/mcp-rs/docs/security/)（HTTPの認証・ツールの制限・TLS・監査ログ）
- [APIリファレンス](https://n-takatsu.github.io/mcp-rs/docs/api/)（`mcp_rs_status`とMCPのメソッド）
- [アーキテクチャ](https://n-takatsu.github.io/mcp-rs/docs/architecture/)
- [Claude Desktop 統合](./project-docs/CLAUDE_DESKTOP_INTEGRATION.md)
- [Claude Code 統合](./project-docs/CLAUDE_CODE_INTEGRATION.md)
- [Dockerガイド](./docs/DOCKER_GUIDE.md)

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
  - `mysql_engine_test.rs`: MySQL データベースエンジンのテスト（`mysql-backend` フィーチャーとMySQLのサーバーが必要）
  - 使用方法: `cargo run --example mysql_engine_test --features database,mysql-backend`

> **注意**: データベースのサーバーが要る例は、CIがそれに依存しないよう`examples.disabled/`に置いています。

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

# <img src="website/assets/favicon.svg" alt="" width="36" height="36"> mcp-rs

AIのクライアントに、WordPress・データベース（PostgreSQL・MySQL・MariaDB）・外部のMCPプラグインのツールを提供する、Model Context Protocol（MCP）のサーバーのRust実装です。

> **[日本語](README.ja.md)** | **[English](README.md)**

[![Version](https://img.shields.io/badge/Version-v0.27.0-blue)](https://github.com/n-takatsu/mcp-rs/releases/tag/v0.27.0)
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

- **JSON-RPC 2.0のMCP**: プロトコルの版は2025-11-25まで（`initialize`を使う）と、2026-07-28（`server/discover`と、リクエストごとの`_meta`。今はSTDIOだけ）
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

## **データベース連携（PostgreSQL・MySQL・MariaDB）**

- **設定は1つの節**: 設定ファイルに`[database]`を書く（`engine = "postgresql"`、または`"mysql"`・`"mariadb"`（v0.25.0以降）、ホスト・データベース・ユーザー、`password = "${VAR}"`）
- **既定は読み取りだけ**: `execute_query`（1つのSELECT。読み取り専用のトランザクションで実行してロールバック）と`get_schema`
- **書き込みは許したときだけ**: `allow_write = true`のときだけ`execute_command`（1つのINSERT・UPDATE・DELETE）を出す
- **バインドするパラメーター**: `$1`・`$2`…（PostgreSQL）や`?`（MySQL・MariaDB）はバインドし、SQLの文字列には埋め込まない。文字列は列の型に変換する
- **上限**: 既定で、結果は1000行、1つのSQLは30秒、接続は5つまで
- **監査ログ**: 呼び出しをSQLとともに記録する（パラメーターの値は記録しない）

読み取り専用のロールで接続してください。詳しくは[Database Integration Guide](https://n-takatsu.github.io/mcp-rs/docs/database/)を参照してください。

## 🚀 クイックスタート

HTTPSでつながる、アプリケーションパスワード（ユーザー → プロフィール → アプリケーションパスワード）を使えるWordPressのサイトが要ります。Rustは要りません。v0.25.0から、各リリースに、Windows（x64）・macOS（Apple Silicon）・Linux（x64）向けのビルド済みのmcp-rsが付きます。

**1. インストール**（v0.26.0から）。スクリプトが、最新のリリースをダウンロードしてSHA256を確かめ、`~/.mcp-rs/bin`（Windowsは`%USERPROFILE%\.mcp-rs\bin`）に置きます。管理者の権限は要らず、ほかには何も変えません。更新も、もう一度動かすだけです。

```powershell
# Windows（PowerShell）
irm https://raw.githubusercontent.com/n-takatsu/mcp-rs/main/install/install.ps1 | iex
```

```bash
# macOS（Apple Silicon）・Linux（x64）
curl -fsSL https://raw.githubusercontent.com/n-takatsu/mcp-rs/main/install/install.sh | sh
```

**2. セットアップ** スクリプトが`mcp-rs init`を動かすかを聞きます（自分で動かしても構いません）。サイトのURL・ユーザー名・アプリケーションパスワードを聞き（ファイルではなく、OSの資格情報ストアに入れます）、設定ファイル（`~/.config/mcp-rs/config.toml`）を書き、接続とサインインを確かめ、Claude Desktop・Claude Code・GitHub Copilot（VS Code）に登録します。変える前には、1つずつ確かめます。

```bash
~/.mcp-rs/bin/mcp-rs init        # Windows: & "$env:USERPROFILE\.mcp-rs\bin\mcp-rs.exe" init
```

**3. 確かめる。** `init`が表示するとおりにクライアントを再起動し、`mcp_rs_status`のツールを呼ぶよう頼みます。版・使っている設定ファイル・ログの場所と、起動に失敗したものが分かります。クライアントを使わずに設定ファイルを確かめるには、`mcp-rs check --config <パス>`を使います。

ほかのこと（スクリプトを読んでから動かす方法、手で入れる・ソースからビルドする方法、クライアントごとの手での設定、設定ファイル、ログの場所、クライアントがツールを見つけられないときに見るところ）は、[Getting Startedのガイド](https://n-takatsu.github.io/mcp-rs/docs/guides/)（英語）にあります。

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

- [Getting Started](https://n-takatsu.github.io/mcp-rs/docs/guides/)（入れ方、クライアントごとの設定、ログファイル）
- [WordPress](https://n-takatsu.github.io/mcp-rs/docs/wordpress/)
- [データベース（PostgreSQL・MySQL・MariaDB）](https://n-takatsu.github.io/mcp-rs/docs/database/)
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

例は`examples/`ディレクトリにあります。例えば`cargo run --example wordpress_api_test`は、`WORDPRESS_URL`のWordPressにつながるかを確かめます。`WORDPRESS_USERNAME`・`WORDPRESS_PASSWORD`も設定すると、ログインを確かめ、下書きの投稿を1件作ります。

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

---

❤️ と Rust 🦀 で構築

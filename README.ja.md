# <img src="website/assets/favicon.svg" alt="" width="36" height="36"> mcp-rs

AIのクライアントに、WordPress・データベース（PostgreSQL・MySQL・MariaDB・SQLite）・外部のMCPプラグインのツールを提供する、Model Context Protocol（MCP）のサーバーのRust実装です。

Claudeに頼んで、WordPressの記事の下書き・予約投稿・公開、メディアのアップロード、コメントの承認ができます。ScoopかHomebrewで入れて、`mcp-rs init`の質問に答えれば、Claude Desktop・Claude Code・GitHub Copilotに登録されます。

> **📖[日本語](README.ja.md)** | **[English](README.md)**

[![Version](https://img.shields.io/badge/Version-v0.30.0-blue)](https://github.com/n-takatsu/mcp-rs/releases/tag/v0.30.0)
[![Rust](https://img.shields.io/badge/dynamic/toml?url=https%3A%2F%2Fraw.githubusercontent.com%2FRedRing2020%2FRedRing%2Fdevelop%2Frust-toolchain.toml&query=%24.toolchain.channel&label=rust&logo=rust&color=orange)](rust-toolchain.toml)
![Implementation](https://img.shields.io/badge/WordPress_Tools-41_Available-green)
![License](https://img.shields.io/badge/License-MIT%2FApache--2.0-green)

## 概要

`mcp-rs`はRustで書かれたMCPのサーバーです。Claude Desktop・Claude Code等のMCPのクライアントはSTDIOで、それ以外のクライアントはHTTPでつなぎます。次の機能があります。

- WordPress REST APIを使う41のツール（投稿・固定ページ・カスタム投稿タイプ・メディア・カテゴリ・タグ等のタクソノミー・コメント・サイト設定）
- 1つのPostgreSQL・MySQL・MariaDB・SQLiteのデータベースを読むSQLのツール（書き込みは許したときだけ）
- プラグインとして起動した外部のMCPサーバーのツール（子プロセス・Dockerのコンテナ・KubernetesのJob）
- HTTPモード向け: APIキー・OAuth 2.1の認証、呼び出し元ごとのツールの制限、TLS・mTLS、レート制限、監査ログ

## 🚀 主要機能

## **コア機能**

- **JSON-RPC 2.0のMCP**: プロトコルの版は2025-11-25まで（`initialize`を使う）と、2026-07-28（`server/discover`と、リクエストごとの`_meta`。STDIOとHTTPの`/mcp`）
- **トランスポート**: STDIOと、HTTP（`/ws`のWebSocketも任意で）
- **プラグイン**: STDIOで話す任意のMCPサーバーのツールを加える。子プロセス・Dockerのコンテナ・KubernetesのJobで起動する。[Plugins](https://n-takatsu.github.io/mcp-rs/docs/plugins/)を参照
- **設定**: 1つのTOMLファイル。`${VAR}`で環境変数の値を使える
- **状態のツール**: `mcp_rs_status`で、バージョン・設定ファイル・ログのファイル・起動しなかったものが分かる

## **WordPress 統合（41 ツール）**

- **投稿・固定ページ管理**: 作成・取得（編集用の原文を全文）・更新・削除。SEO メタデータと予約投稿に対応。リビジョンの一覧と書き戻し
- **メディア管理**: base64 でのアップロード（形式・拡張子・内容を検証）、一覧、メタデータ更新、削除、アイキャッチ画像の設定
- **カテゴリ・タグ管理**: 作成・更新・削除。カテゴリの階層に対応
- **コメント**: 状態（承認待ち・スパム等）ごとの一覧・承認・スパム・返信・削除
- **YouTube・ソーシャル埋め込み**: セキュリティ検証付きリッチメディア統合
- **サイト設定**: サイト設定とフロントページ設定の取得・更新
- **ヘルスチェック**: 接続・認証・権限・メディアアップロード可否の確認
- **カスタム投稿タイプ**: 商品・イベント等、サイトが REST API に出している投稿タイプを、`post_types` で許したときに使う。独自のタクソノミーの項目も付けられる（[文書](https://n-takatsu.github.io/mcp-rs/docs/wordpress/#custom-post-types)）
- **プロンプト**: クライアントのメニューから選べる定型の頼み方（記事の下書き・SEOの見直し・コメントの処理、データベースの表の要約）
- **Claude Desktop の拡張機能**: リリースの `mcp-rs-<版>.mcpb` を開いて入れ、サイトは Claude Desktop の設定画面で入力する（[文書](https://n-takatsu.github.io/mcp-rs/docs/guides/#claude-desktop-extension)）。[公式の MCP Registry](https://registry.modelcontextprotocol.io/v0/servers?search=io.github.n-takatsu/mcp-rs) に `io.github.n-takatsu/mcp-rs` として載っている
- **複数のサイト**: 1つの mcp-rs で複数の WordPress サイトを使う。各ツールの `site` の引数で選ぶ（[文書](https://n-takatsu.github.io/mcp-rs/docs/wordpress/#sites)）

### ライブデモサイト

`examples/wordpress_site_builder_demo.rs` の例は、公開 WordPress デモサイト [redring.jp](https://redring.jp/) を構築・更新します。mcp-rs がサイトを立ち上げ、コンテンツを整理して実運用する流れを示す実例です。

## **セキュリティ**

- **認証情報をファイルに書かない**: `mcp-rs init`は、WordPressのApplication Passwordとデータベースのパスワードを、OSの資格情報ストアに入れる。WordPressの資格情報は、AES-GCM-256（PBKDF2で鍵を導出）で暗号化して保存することもできる。どちらのパスワードも、環境変数から読むこともできる
- **HTTPモードの認証**: APIキー、または外部の認可サーバーが発行したOAuth 2.1のアクセストークン（JWT）
- **呼び出し元ごとのツールの制限**: APIキー・トークンごとに、使えるツールを絞れる（例: `@read_only`）
- **TLS・mTLS**: 最低のTLSバージョンを指定したHTTPS、クライアント証明書（任意）
- **ネットワークポリシー・リプレイ攻撃の防止・侵入検知**（HTTP・WebSocketの待ち受け）
- **レート制限**（ツールの呼び出し）
- **監査ログ**: 誰がどのツールを呼び、結果がどうだったか

## **データベース連携（PostgreSQL・MySQL・MariaDB・SQLite）**

- **設定は1つの節**: 設定ファイルに`[database]`を書く（`engine = "postgresql"`、または`"mysql"`・`"mariadb"`（v0.25.0以降）、ホスト・データベース・ユーザー、`password = "${VAR}"`。SQLite（v0.31.0以降）は、`engine = "sqlite"`とデータベースのファイルのパス）
- **既定は読み取りだけ**: `execute_query`（1つのSELECT。読み取り専用のトランザクションで実行してロールバック）と`get_schema`
- **書き込みは許したときだけ**: `allow_write = true`のときだけ`execute_command`（1つのINSERT・UPDATE・DELETE）を出す
- **バインドするパラメーター**: `$1`・`$2`…（PostgreSQL）や`?`（MySQL・MariaDB・SQLite）はバインドし、SQLの文字列には埋め込まない。文字列は列の型に変換する
- **上限**: 既定で、結果は1000行、1つのSQLは30秒、接続は5つまで
- **監査ログ**: 呼び出しをSQLとともに記録する（パラメーターの値は記録しない）

読み取り専用のロールで接続してください。詳しくは[Database Integration Guide](https://n-takatsu.github.io/mcp-rs/docs/database/)を参照してください。

## 🚀 クイックスタート

WordPressを使うには、HTTPSでつながる、アプリケーションパスワード（ユーザー → プロフィール → アプリケーションパスワード）を使えるWordPressのサイトが要ります。データベース（PostgreSQL・MySQL・MariaDB）だけを使うなら、WordPressのサイトは要りません。`mcp-rs init`の最初の質問で「2. A database」を選びます。Rustは要りません。v0.25.0から、各リリースに、Windows（x64）・macOS（Apple Silicon）・Linux（x64）向けのビルド済みのmcp-rsが付きます。

**1. インストール** パッケージマネージャーで入れます（v0.29.0から）。リリースのビルド済みのmcp-rsを入れ、ほかのパッケージと一緒に更新できます（`scoop update`・`brew upgrade`）。

```powershell
# Windows（Scoop）
scoop bucket add n-takatsu https://github.com/n-takatsu/scoop-bucket
scoop install n-takatsu/mcp-rs
```

```bash
# macOS（Apple Silicon）・Linux（x64）（Homebrew）
brew install n-takatsu/tap/mcp-rs
```

インストールスクリプトでも入れられます（v0.26.0から）。スクリプトが、最新のリリースをダウンロードしてSHA256を確かめ、`~/.mcp-rs/bin`（Windowsは`%USERPROFILE%\.mcp-rs\bin`）に置きます。管理者の権限は要らず、ほかには何も変えません。更新も、もう一度動かすだけです。

```powershell
# Windows（PowerShell）
irm https://raw.githubusercontent.com/n-takatsu/mcp-rs/main/install/install.ps1 | iex
```

```bash
# macOS（Apple Silicon）・Linux（x64）
curl -fsSL https://raw.githubusercontent.com/n-takatsu/mcp-rs/main/install/install.sh | sh
```

**2. セットアップ** `mcp-rs init`を動かします（インストールスクリプトは、動かすかを聞きます）。サイトのURL・ユーザー名・アプリケーションパスワードを聞き（ファイルではなく、OSの資格情報ストアに入れます。データベースも、WordPressと一緒にでも、データベースだけでも設定できます。データベースだけなら、WordPressのことは聞きません）、設定ファイル（`~/.config/mcp-rs/config.toml`）を書き、接続とサインインを確かめ、Claude Desktop・Claude Code・GitHub Copilot（VS Code）に登録します。変える前には、1つずつ確かめます。

```bash
mcp-rs init
# スクリプトで入れた場合: ~/.mcp-rs/bin/mcp-rs init
# （Windows: & "$env:USERPROFILE\.mcp-rs\bin\mcp-rs.exe" init）
```

**3. 確かめる。** `init`が表示するとおりにクライアントを再起動し、`mcp_rs_status`のツールを呼ぶよう頼みます。版・使っている設定ファイル・ログの場所と、起動に失敗したもの、新しい版が出ているかが分かります。クライアントを使わずに設定ファイルを確かめるには、`mcp-rs check --config <パス>`を使います。

`init`が設定したもの（クライアントへの登録、資格情報ストアの項目、選べば設定ファイル）を外すには、`mcp-rs uninstall`を動かします。変える前に1つずつ確かめ、入れ方に合わせたmcp-rs自身の消し方も示します。

ほかのこと（スクリプトを読んでから動かす方法、手で入れる・ソースからビルドする方法、クライアントごとの手での設定、設定ファイル、ログの場所、クライアントがツールを見つけられないときに見るところ）は、[Getting Startedのガイド](https://n-takatsu.github.io/mcp-rs/docs/guides/)（英語）にあります。

## 🏗️ アーキテクチャ

![mcp-rsの構成: AIのクライアントがSTDIOかHTTPでつなぎ、mcp-rsが呼び出しを確かめて、WordPressのサイト・データベース・プラグインのツールを提供する](https://raw.githubusercontent.com/n-takatsu/mcp-rs/develop/website/assets/architecture.svg)

## 📚 ドキュメント

ドキュメントのサイトは[n-takatsu.github.io/mcp-rs](https://n-takatsu.github.io/mcp-rs/docs/)です（英語）。

- [Getting Started](https://n-takatsu.github.io/mcp-rs/docs/guides/)（入れ方、クライアントごとの設定、ログファイル）
- [WordPress](https://n-takatsu.github.io/mcp-rs/docs/wordpress/)
- [データベース（PostgreSQL・MySQL・MariaDB・SQLite）](https://n-takatsu.github.io/mcp-rs/docs/database/)
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

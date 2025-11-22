# mcp-rs

🚀 **本番対応** Model Context Protocol (MCP) の Rust 実装。WordPress とそれ以外のシステムとの AI エージェント統合を実現します。

> **[日本語](README.ja.md)** | **[English](README.md)**

[![Version](https://img.shields.io/badge/Version-v0.15.0-blue)](https://github.com/n-takatsu/mcp-rs/releases/tag/v0.15.0)
![Architecture](https://img.shields.io/badge/Architecture-Production--Ready-green)
![Implementation](https://img.shields.io/badge/WordPress_Tools-27_Available-green)
![License](https://img.shields.io/badge/License-MIT%2FApache--2.0-green)

## 概要

`mcp-rs` は、**包括的で実戦テスト済み**の MCP（Model Context Protocol）の Rust 実装で、**完全な WordPress 統合**を提供します。レイヤード・アーキテクチャで構築され、AI エージェントが標準化された JSON-RPC インターフェースを通じて高度なコンテンツ管理を実行できるようにします。このフレームワークは 27 の WordPress ツール、エンタープライズレベルのセキュリティ、強力な型安全性を備え、GitHub Copilot や他の AI コーディングアシスタントでの本番利用に最適化されています。

## 🎯 対象ユーザー

## **AI 開発者** 🤖

Claude Desktop アプリ、GPT 統合、カスタム AI エージェントを構築していますか？ mcp-rs は包括的な WordPress ツールを備えた本番対応の Model Context Protocol 実装を提供します。

## **エンタープライズ WordPress チーム** 🏢

大規模な WordPress デプロイメントを管理していますか？エンタープライズグレードのセキュリティ、自動化されたコンテンツ管理、シームレスな CI/CD 統合を取得できます。

## **DevOps エンジニア** ⚙️

WordPress 運用を自動化していますか？包括的なヘルスチェック、監視、本番対応のエラーハンドリングを備えた 27 の実戦テスト済みツールを利用できます。

## **Rust 愛好者** 🦀

高品質な Rust コードベースに貢献したいですか？クリーンなアーキテクチャと包括的なドキュメントを備えた、205+ テスト、警告ゼロのプロジェクトにご参加ください。

## **セキュリティチーム** 🔒

WordPress セキュリティ自動化が必要ですか？SQL インジェクション保護、XSS 防御、包括的な監査ログを備えた 6 層エンタープライズセキュリティアーキテクチャをご利用ください。

## 🚀 主要機能

## **コア機能**

- **JSON-RPC 2.0 サーバー**: `axum` を使用したフル機能 JSON-RPC サーバー実装
- **マルチトランスポート対応**: STDIO、HTTP、WebSocket 通信プロトコル
- **プラグインアーキテクチャ**: `McpHandler` トレイトによる拡張可能なハンドラーベースシステム
- **型安全な設定**: 環境変数オーバーライドを備えた TOML ベース設定
- **本番対応エラーハンドリング**: 構造化ログを備えた包括的なエラータイプ
- **Async/Await**: 高性能非同期操作のための `tokio` ベース

## **WordPress 統合（27 ツール）**

- **投稿・固定ページ管理**: SEO 統合を含む完全な CRUD 操作
- **高度なメディア管理**: base64 サポート付きのアップロード、リサイズ、整理
- **カテゴリ・タグ管理**: 一括操作を伴う階層サポート
- **コメントシステム**: 完全なコメント管理と取得
- **YouTube・ソーシャル埋め込み**: セキュリティ検証付きリッチメディア統合
- **ユーザー管理**: ロールベースアクセス制御とユーザー操作

## **エンタープライズセキュリティ（6 層アーキテクチャ）**

- **AES-GCM-256 暗号化**: PBKDF2 鍵導出による軍用グレード暗号化
- **SQL インジェクション保護**: 11 の攻撃パターンのリアルタイム検出
- **XSS 防御**: 14 の XSS 攻撃ベクトルに対する高度な保護
- **レート制限**: DDoS 保護付きトークンバケットアルゴリズム
- **TLS 強制**: 証明書検証付き TLS 1.2+
- **監査ログ**: 包括的なセキュリティイベント追跡

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
| **セキュリティテスト** | 6 層検証 |
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

### オプション 1: 対話式設定（推奨）

```bash

## リポジトリをクローン

git clone https://github.com/n-takatsu/mcp-rs.git
cd mcp-rs

## リリース版をビルド

cargo build --release

## 対話式設定セットアップを実行

./target/release/mcp-rs --setup-config
```

対話式設定の特徴:

- 📝 ユーザーフレンドリーな質問形式
- 🔍 リアルタイム接続テスト
- ⚡ 自動設定ファイル生成
- 🛡️ セキュリティ設定の推奨

### オプション 2: 手動設定

```bash

## サンプル設定ファイルを生成

./target/release/mcp-rs --generate-config

## 設定を編集

cp mcp-config.toml.example mcp-config.toml

## mcp-config.toml を WordPress の詳細で編集

## カスタム設定で実行

./target/release/mcp-rs --config mcp-config.toml
```

## 基本設定

`mcp-config.toml` を作成:

```toml
[wordpress]
base_url = "https://your-wordpress-site.com"
username = "your-username"
password = "your-application-password"  

## WordPress アプリケーションパスワード

[server]
transport_type = "stdio"  

## Claude Desktop 用

## transport_type = "http"  # Web UI 用

## bind_addr = "127.0.0.1:8080"  

## HTTP モードのみ

[logging]
level = "error"  

## Claude Desktop 用の最小ログ

## level = "info"  # 開発用の詳細ログ

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
                           │ • SQL インジェクション │
                           │ • XSS 保護          │
                           │ • レート制限        │
                           │ • 暗号化            │
                           └─────────────────────┘
```

## 📚 ドキュメント

- [🚀 クイックスタートガイド](./project-docs/quick-start.md)
- [🔧 設定リファレンス](./project-docs/configuration.md)
- [🔒 セキュリティガイド](./project-docs/security.md)
- [🏗️ アーキテクチャ概要](./project-docs/architecture.md)
- [📝 API リファレンス](./project-docs/api-reference.md)
- [🔄 Claude Desktop 統合](./project-docs/CLAUDE_DESKTOP_INTEGRATION.md)

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

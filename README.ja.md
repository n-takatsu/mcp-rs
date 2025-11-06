# 🚀 WordPress MCP Server

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://rustlang.org)
[![Tests](https://img.shields.io/badge/tests-205%20passing-green.svg)](https://github.com/n-takatsu/mcp-rs)

> **日本語** | [English](README.md)

エンタープライズグレードのWordPress Model Context Protocol (MCP) サーバー。AIシステムと自動化ワークフロー向けの安全で高性能なWordPress統合を提供します。

## 🎯 概要

このMCPサーバーは、WordPressサイトとの包括的な統合機能を提供し、コンテンツ作成、メディア管理、セキュリティ監視を自動化します。軍事レベルの暗号化と多層防御セキュリティアーキテクチャを特徴としています。

## ✨ 主要機能

### 🔧 WordPress統合（27ツール）
- **投稿・固定ページ管理**: SEO統合を含むCRUD操作
- **高度なメディア管理**: base64サポートによるアップロード、リサイズ、整理
- **カテゴリ・タグ管理**: 階層サポートとバルク操作
- **コメントシステム**: 完全なコメント管理と取得
- **YouTube・ソーシャル埋め込み**: セキュリティ検証付きリッチメディア統合
- **ユーザー管理**: ロールベースアクセス制御とユーザー操作

### 🛡️ エンタープライズセキュリティ（6層アーキテクチャ）
- **AES-GCM-256暗号化**: PBKDF2鍵導出による軍事レベル暗号化
- **SQLインジェクション保護**: 11種類の攻撃パターンのリアルタイム検出
- **XSS防止**: 14種類のXSS攻撃ベクターに対する高度な保護
- **レート制限**: DDoS保護付きトークンバケットアルゴリズム
- **TLS強制**: 証明書検証付きTLS 1.2+
- **監査ログ**: 包括的なセキュリティイベント追跡

### 🏗️ 技術的優秀性
- **非同期アーキテクチャ**: 高性能並行処理のためのTokio構築
- **型安全性**: 100%メモリ安全なRust実装
- **JSON-RPC 2.0**: axumフレームワークを使用した標準準拠サーバー
- **🔄 動的データベース切り替え**: ゼロダウンタイムでのデータベースエンジン切り替え
- **包括的テスト**: 100%合格率の205+テスト
- **警告ゼロ**: clippyワーニングゼロのクリーンなコードベース
- **本番対応**: 最適化されたビルドプロファイルとエラーハンドリング

## 📊 品質指標

| 指標 | 値 |
|------|-----|
| **総テスト数** | 205+ (100%合格) |
| **コードカバレッジ** | 包括的 |
| **セキュリティテスト** | 6層検証 |
| **パフォーマンス** | 本番最適化 |
| **ドキュメント** | 完全なAPIドキュメント |

## 🚀 クイックスタート

### 前提条件
- Rust 1.70+ (2021 edition)
- アプリケーションパスワードが有効なWordPressサイト
- WordPress REST APIへのネットワークアクセス

### インストール
```bash
# リポジトリをクローン
git clone https://github.com/n-takatsu/mcp-rs.git
cd mcp-rs

# リリースビルド
cargo build --release

# 設定して実行
cp mcp-config.toml.example mcp-config.toml
# mcp-config.tomlをWordPress認証情報で編集
cargo run --release
```

### 設定
```toml
[wordpress]
base_url = "https://your-wordpress-site.com"
username = "your-username"
application_password = "your-app-password"

[security]
encryption_enabled = true
rate_limit_enabled = true
sql_injection_protection = true
xss_protection = true
audit_logging = true
```

### WordPressアプリケーションパスワード設定

1. WordPressダッシュボードにログイン
2. **ユーザー** → **プロフィール** に移動
3. **アプリケーションパスワード** セクションまでスクロール
4. 新しいアプリケーション名を入力（例：「MCP Server」）
5. **新しいアプリケーションパスワードを追加** をクリック
6. 生成されたパスワードを `mcp-config.toml` にコピー

## 🎯 対象ユーザー

- **WordPress開発者**: コンテンツ作成とサイト管理の自動化
- **エンタープライズチーム**: ビジネスワークフロー向けの安全なWordPress統合
- **AIシステム**: コンテンツ自動化のための信頼性の高いWordPress統合
- **コンテンツクリエーター**: 合理化された公開とメディア管理

## 🔧 利用可能なツール

### コンテンツ管理
- `wordpress_create_post` - 新しい投稿作成
- `wordpress_update_post` - 既存投稿更新
- `wordpress_delete_post` - 投稿削除
- `wordpress_get_post` - 投稿取得
- `wordpress_list_posts` - 投稿一覧取得
- `wordpress_create_page` - 新しい固定ページ作成
- `wordpress_update_page` - 固定ページ更新
- `wordpress_get_page` - 固定ページ取得

### メディア管理
- `wordpress_upload_media` - メディアファイルアップロード
- `wordpress_get_media` - メディア情報取得
- `wordpress_list_media` - メディアライブラリ一覧
- `wordpress_delete_media` - メディア削除
- `wordpress_update_media` - メディア情報更新

### カテゴリ・タグ
- `wordpress_create_category` - 新しいカテゴリ作成
- `wordpress_list_categories` - カテゴリ一覧取得
- `wordpress_create_tag` - 新しいタグ作成
- `wordpress_list_tags` - タグ一覧取得

### コメント管理
- `wordpress_list_comments` - コメント一覧取得
- `wordpress_get_comment` - 個別コメント取得
- `wordpress_create_comment` - 新しいコメント作成
- `wordpress_update_comment` - コメント更新
- `wordpress_delete_comment` - コメント削除

### 高度な機能
- `wordpress_search_content` - コンテンツ検索
- `wordpress_bulk_operations` - バルク操作
- `wordpress_seo_analysis` - SEO分析
- `wordpress_backup_content` - コンテンツバックアップ
- `wordpress_security_scan` - セキュリティスキャン

## 🔄 動的データベース切り替え機能

### エンタープライズ機能
MCP-RSは業界最先端の**ゼロダウンタイム データベースエンジン切り替え機能**を提供します。

#### 主要機能
- **⚡ ゼロダウンタイム切り替え**: サービス中断なしでのデータベースエンジン変更
- **🔄 リアルタイム監視**: 全エンジンの健康状態とパフォーマンスの継続監視
- **🛡️ 自動フェイルオーバー**: 障害検出時の自動切り替え
- **📊 パフォーマンス最適化**: ワークロード特性に基づく最適エンジンの自動選択
- **🔧 ホット設定**: サービス再起動なしでの設定変更

#### サポート対象エンジン組み合わせ
| プライマリ | セカンダリ | 用途 |
|-----------|----------|------|
| PostgreSQL | Redis, MongoDB | 高スループットWebアプリ |
| MySQL | Redis, SQLite | 従来型Webスタック + キャッシュ |
| MongoDB | PostgreSQL, Redis | ドキュメント重視 + リレーショナル |
| Redis | PostgreSQL, MongoDB | キャッシュファースト + 永続化 |

#### MCPツール
```bash
# データベースエンジン切り替え
switch_database_engine --target postgresql --mode graceful

# 自動切り替えポリシー設定
configure_switch_policy --trigger performance --threshold 1000ms

# リアルタイムエンジンメトリクス取得
get_engine_metrics --engine all --format json
```

## 🔒 セキュリティ機能

### 暗号化
- **AES-GCM-256**: 認証付き暗号化モード
- **PBKDF2**: 100,000回反復による鍵導出
- **安全な鍵管理**: メモリ内での安全な鍵処理
- **パスワード難読化**: 平文パスワードの漏洩防止

### 攻撃防御
- **SQLインジェクション**: 11種類の攻撃パターン検出
- **XSS保護**: 14種類のクロスサイトスクリプティング防止
- **入力検証**: 包括的なデータサニタイゼーション
- **レート制限**: 設定可能なリクエスト制限

### 監視・ログ
- **リアルタイム監視**: セキュリティイベントの即座の検出
- **構造化ログ**: JSON形式での詳細ログ記録
- **メトリクス収集**: パフォーマンスと使用状況の追跡
- **アラート機能**: 異常検出時の通知

## 🧪 テスト

```bash
# 全テスト実行
cargo test

# リリースモードテスト
cargo test --release

# 詳細出力付きテスト
cargo test -- --nocapture

# ベンチマーク実行
cargo bench
```

### テストカバレッジ
- **ユニットテスト**: 93件 (コア機能)
- **統合テスト**: 82件 (メイン機能)
- **システムテスト**: 30件 (エンドツーエンド)
- **セキュリティテスト**: 全セキュリティ機能カバー

## 📚 ドキュメント

- **[プロジェクトドキュメント](project-docs/)**: 技術仕様とアーキテクチャ
- **[セキュリティガイド](project-docs/security-guide.md)**: セキュリティ実装の詳細
- **[アーキテクチャ](project-docs/architecture.md)**: システム設計ドキュメント
- **[WordPress ガイド](project-docs/wordpress-guide.md)**: WordPress統合の詳細
- **[貢献ガイドライン](CONTRIBUTING.md)**: 開発参加方法

## 🤝 貢献

プロジェクトへの貢献を歓迎します！

1. リポジトリをフォーク
2. 機能ブランチを作成 (`git checkout -b feature/amazing-feature`)
3. 変更をコミット (`git commit -m 'Add amazing feature'`)
4. ブランチにプッシュ (`git push origin feature/amazing-feature`)
5. プルリクエストを開く

詳細は [CONTRIBUTING.md](CONTRIBUTING.md) をご覧ください。

## 📄 ライセンス

このプロジェクトは MIT OR Apache-2.0 のデュアルライセンスです。

- [MIT License](LICENSE-MIT)
- [Apache License 2.0](LICENSE-APACHE)

## 🔗 リンク

- **[GitHubリポジトリ](https://github.com/n-takatsu/mcp-rs)**
- **[リリース](https://github.com/n-takatsu/mcp-rs/releases)**
- **[Issue報告](https://github.com/n-takatsu/mcp-rs/issues)**
- **[ディスカッション](https://github.com/n-takatsu/mcp-rs/discussions)**

## 🙏 謝辞

- [Model Context Protocol](https://modelcontextprotocol.io/) による革新的なプロトコル仕様
- [WordPress REST API](https://developer.wordpress.org/rest-api/) による包括的なAPI
- [Rust](https://www.rust-lang.org/) による安全で高性能な実装基盤

---

**⭐ このプロジェクトが役立つ場合は、スターをお願いします！**
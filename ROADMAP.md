# 🗺️ mcp-rs プロジェクト ロードマップ

> **最終更新**: 2026年11月16日
> **バージョン**: v1.6
> **ステータス**: アクティブ開発中

## 🎯 プロジェクト概要

mcp-rsは、エンタープライズグレードのWordPress Model Context Protocol (MCP) サーバーです。AI システムと WordPress サイトの安全で高性能な統合を提供します。

## 🏆 プロジェクト目標

- **セキュリティファースト**: 軍事レベルの暗号化と多層防御
- **高性能**: 非同期アーキテクチャによる最適化
- **拡張性**: プラグインシステムによる機能拡張
- **信頼性**: 100% メモリ安全な Rust 実装

---

## 📜 これまでの変更

完了した機能・リリースの履歴は、[CHANGELOG.md](CHANGELOG.md)・[RELEASE_NOTES.md](RELEASE_NOTES.md)・[GitHub Releases](https://github.com/n-takatsu/mcp-rs/releases)を参照してください。このロードマップには、これからの計画と、今のマイルストーンで完了した範囲を載せます。

---

## 🚀 リリース計画

## 📦 v1.0.0 (2027年6月頃 - 週2日ペース想定)

**テーマ**: HTTPモードを組織で安全に使えるようにし、用意してあるデータベースを本体から使えるようにする

MCPのクライアント（Claude Desktop・Claude Code等）から使える機能に絞る。2026-11-08に範囲を見直した（#419）。SAML・LDAP・MFA・マルチテナント・請求システム・ゼロコピー/分散キャッシュは対象外とした（SAML・LDAPとの連携は、OAuth 2.1の認可サーバー（IdP）側で行う）。

マイルストーン「v1.0.0」は計画の区切りで、クレートのバージョン（`Cargo.toml`の`version`）とは別の番号。

### ✅ 完了: HTTPモードの認可・ツールの権限・監査ログ（Epic #41）

2026-11-08に完了した（#421・#416・#417・#420・#52・#55）。詳細はCHANGELOG.mdを参照。

### 📅 フェーズ: データベースの対応を広げる（2026-11-13に追加、#526）

データベースのハンドラーは、MySQL・MariaDB・PostgreSQL・SQLite・Redis・MongoDBのエンジンをライブラリとして持っているが、本体（`[database]`）から使えるのはPostgreSQLだけ（#453）。用意してあるエンジンを、PostgreSQLと同じ安全性（既定は読み取りだけ、行数の上限、タイムアウト）で使えるようにする。

- **Phase 4: 土台**（✅ 2026-11-13に完了）
  - #513 サーバーが要る結合テスト（PostgreSQL・Redis）をCIのサービスのコンテナで実行する
- **Phase 5: SQLのデータベース**
  - #527 MySQL・MariaDB
  - #528 SQLite（同梱のSQLiteの脆弱性の扱いを含む。#449 はsqlxの対応待ち）
- **Phase 6: SQLではないデータベース**
  - #529 Redis
  - #530 MongoDB

どのエンジンも、リリースのバイナリ・Dockerイメージに含めるかを、それぞれのIssueで決める（バイナリの大きさと、`cargo audit`の対象が増える）。

### 📅 フェーズ: MCPの2026-07-28版に対応する（2026-11-16に追加、#566）

MCPの仕様の2026-07-28版で、`initialize`の手続きとセッションが無くなった。今のmcp-rs（2025-11-25版）は、新旧の両方に対応したクライアントとは仕様どおりにつながるが、新しい版だけのクライアントとはつながらない。仕様には、`initialize`をいつまで使えるかの決まりが無い。今のクライアントを壊さないよう、新旧の両方を受ける形にする。

- #570 MCPの2026-07-28版（initializeの廃止）に、新旧の両方を受ける形で対応する（STDIO→HTTP→無くなったものの順）

### ✅ 2026-11-15〜16に完了

- #521 VS Code（Copilot）からmcp-rsを使う手順（#563）
- #565 設定を確かめるコマンド`mcp-rs check`（#568）
- #564 リリースに、ビルド済みのmcp-rsを付ける（#569。次のリリースのタグで初めて動く）
- #566 MCPの新しい版（`server/discover`）の調査（#570を起票）

### 📝 そのほか

- #449 sqlxが対応したらRUSTSEC-2026-0459（同梱のSQLiteの脆弱性）の無視を外す（sqlxの対応待ち）

---

## 🔗 関連リソース

## 📋 GitHub Issues

- [Milestone v1.0.0](https://github.com/n-takatsu/mcp-rs/milestone/3) - v1.0.0に入るIssue
- [Epic #41](https://github.com/n-takatsu/mcp-rs/issues/41)（完了） - HTTPモードの認可・ツールの権限・監査ログ

## 📚 ドキュメント

- [Architecture](https://n-takatsu.github.io/mcp-rs/docs/architecture/)
- [Security](https://n-takatsu.github.io/mcp-rs/docs/security/)
- [開発ガイドライン](CONTRIBUTING.md)

## 🎯 プロジェクト管理

- [GitHub Project Board](https://github.com/n-takatsu/mcp-rs/projects)
- [Milestone 管理](https://github.com/n-takatsu/mcp-rs/milestones)
- [Release Notes](https://github.com/n-takatsu/mcp-rs/releases)

---

## 🤝 貢献とフィードバック

## 💡 機能の提案

- [Feature Request](https://github.com/n-takatsu/mcp-rs/issues/new?template=feature_request.yml) で新機能提案

## 🐛 バグ報告

- [Bug Report](https://github.com/n-takatsu/mcp-rs/issues/new?template=bug_report.yml) でバグ報告

## 📊 ロードマップフィードバック

このロードマップに対するご意見・ご要望は、[Issue](https://github.com/n-takatsu/mcp-rs/issues/new/choose)でお寄せください。

---

*このロードマップは生きたドキュメントです。コミュニティのフィードバックと技術的進歩に応じて定期的に更新されます。*

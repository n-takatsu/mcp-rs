# 🗺️ mcp-rs プロジェクト ロードマップ

> **最終更新**: 2026年11月08日
> **バージョン**: v1.4
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

完了した機能・リリースの履歴は、[CHANGELOG.md](CHANGELOG.md)・[RELEASE_NOTES.md](RELEASE_NOTES.md)・[GitHub Releases](https://github.com/n-takatsu/mcp-rs/releases)を参照してください。このロードマップには、これからの計画だけを載せます。

---

## 🚀 リリース計画

## 📦 v1.0.0 (2027年6月頃 - 週2日ペース想定)

**テーマ**: HTTPモードを組織で安全に使えるようにする（Epic #41）

MCPのクライアント（Claude Desktop・Claude Code等）から使える機能に絞る。2026-11-08に範囲を見直した（#419）。SAML・LDAP・MFA・マルチテナント・請求システム・ゼロコピー/分散キャッシュは対象外とした（SAML・LDAPとの連携は、OAuth 2.1の認可サーバー（IdP）側で行う）。

### 📅 フェーズ

- **Phase 1: 準備**
  - #421 本体から使われていない認証コードの削除
  - #416 監査ログの書き込み先を設定で指定できるようにする
  - #417 MCPのプロトコル版を新しい版（2025-06-18）に対応させる
- **Phase 2: 認可とツールの権限**
  - #420 MCPの仕様のOAuth 2.1による認可（mcp-rsはリソースサーバーとしてトークンを検証する）
  - #52 APIキー・トークンごとに、使えるツールを制限する
- **Phase 3: 監査ログ**
  - #55 監査ログに、呼び出し元（APIキー・トークン）と結果を記録する

---

## 🔗 関連リソース

## 📋 GitHub Issues

- [Epic #41](https://github.com/n-takatsu/mcp-rs/issues/41)（Open） - v1.0.0: HTTPモードの認可・ツールの権限・監査ログ

## 📚 ドキュメント

- [アーキテクチャ設計](../project-docs/architecture.md)
- [セキュリティガイド](../project-docs/security-guide.md)
- [開発ガイドライン](../CONTRIBUTING.md)

## 🎯 プロジェクト管理

- [GitHub Project Board](https://github.com/n-takatsu/mcp-rs/projects)
- [Milestone 管理](https://github.com/n-takatsu/mcp-rs/milestones)
- [Release Notes](https://github.com/n-takatsu/mcp-rs/releases)

---

## 🤝 貢献とフィードバック

## 💬 ディスカッション

- [GitHub Discussions](https://github.com/n-takatsu/mcp-rs/discussions) で機能提案や質問
- [Feature Request](https://github.com/n-takatsu/mcp-rs/issues/new?template=feature_request.yml) で新機能提案

## 🐛 バグ報告

- [Bug Report](https://github.com/n-takatsu/mcp-rs/issues/new?template=bug_report.yml) でバグ報告

## 📊 ロードマップフィードバック

このロードマップに対するご意見・ご要望は [Roadmap Discussion](https://github.com/n-takatsu/mcp-rs/discussions/categories/roadmap) までお寄せください。

---

## 📈 メトリクスと目標

## 🎯 2026年 Q1 目標

- **コントリビューター**: 3-5名
- **GitHub Stars**: 50+
- **プロダクションユーザー**: 5+組織

## 📊 長期目標 (2026年)

- **エンタープライズ導入**: 50+組織
- **コミュニティサイズ**: 500+開発者
- **WordPress プラグインエコシステム**: 20+プラグイン対応

---

*このロードマップは生きたドキュメントです。コミュニティのフィードバックと技術的進歩に応じて定期的に更新されます。*

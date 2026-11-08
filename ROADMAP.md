# 🗺️ mcp-rs プロジェクト ロードマップ

> **最終更新**: 2026年11月08日
> **バージョン**: v1.3
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

**テーマ**: 本番環境完全対応（Epic #41、進捗10%・実装未着手）

**ペース**: 週2日程度の作業を前提に再設定（元の2026年8月締切は未着手のまま経過したため）

### 📅 フェーズ目標（実装未着手から再スタート）

- **Phase 1: エンタープライズ認証** — 2027年1月頃
- **Phase 2: パフォーマンス最適化** — 2027年3月頃
- **Phase 3: マルチテナント完成** — 2027年6月頃

### 🎯 主要機能

- **🏢 エンタープライズ機能**
  - SAML/OAuth2 統合
  - ロールベースアクセス制御
  - 監査ログとコンプライアンス

- **⚡ パフォーマンス最適化**
  - ゼロコピー最適化
  - 分散キャッシュ
  - 負荷分散サポート

- **🌐 マルチテナント**
  - テナント間完全分離
  - リソース配分制御
  - 統合請求システム

---

## 🎯 優先機能ランキング

v1.0.0（Epic #41、本番環境完全対応）に向けた残課題。

## 🔥 P1 (High)

1. **エンタープライズ認証**（SAML/OAuth2・RBAC・監査ログ）

## 📊 P2 (Medium)

2. **パフォーマンス最適化**（ゼロコピー・分散キャッシュ・負荷分散）

## 🔮 P3 (Low)

3. **マルチテナント機能**
4. **カスタム拡張API**

---

## 🔗 関連リソース

## 📋 GitHub Issues

- [エンタープライズ機能 Epic #41](https://github.com/n-takatsu/mcp-rs/issues/41)（Open） - 認証・マルチテナント（v1.0.0）

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

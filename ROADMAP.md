# 🗺️ mcp-rs プロジェクト ロードマップ

> **最終更新**: 2026年11月19日
> **バージョン**: v1.10
> **ステータス**: アクティブ開発中

## 🎯 プロジェクト概要

mcp-rsは、Rustで書いたModel Context Protocol (MCP) サーバーです。Claude Desktop・Claude Code・VS Code（Copilot）等のAIクライアントから、WordPressのサイトと、データベース（PostgreSQL・MySQL・MariaDB）を安全に使えるようにします。

## 🏆 プロジェクト目標

2026-11-17に、今のmcp-rsに合わせて書き直した。

- **安全が既定**: AIのクライアントに渡す権限を、既定で小さくする。データベースは読み取りだけ・上限付き、HTTPモードは認証が要る、ツールの権限と監査ログ、SQLの検査
- **導入が簡単**: ビルド済みのファイル、インストールスクリプト、パッケージマネージャー、`mcp-rs init`、`mcp-rs check`で、設定ファイルを手で書かずに使い始められる
- **MCPのクライアントで使えるものに絞る**: Claude Desktop・Claude Code・VS Code（Copilot）から使える機能を作る（#419で範囲を見直した）
- **確かめてから出す**: CIで、実際のデータベース（PostgreSQL・MySQL・MariaDB）と3つのOSで確かめる。Rustで書く

---

## 📜 これまでの変更

完了した機能・リリースの詳細は、[CHANGELOG.md](CHANGELOG.md)・[RELEASE_NOTES.md](RELEASE_NOTES.md)・[GitHub Releases](https://github.com/n-takatsu/mcp-rs/releases)を参照してください。このロードマップには、これからの計画と、作業中の版を載せます（2026-11-19に、完了した版を1行ずつにまとめた）。

| 版 | 日付 | 内容 |
|---|---|---|
| Epic #41 | 2026-11-08 | HTTPモードの認可・ツールの権限・監査ログ |
| v0.25.0・v0.26.0 | 2026-11-17 | 導入を簡単にする: ビルド済みのファイル（#564）、`mcp-rs check`（#565）、`mcp-rs init`（#575）、インストールスクリプト（#576）、MySQL・MariaDB（#527） |
| v0.27.0 | 2026-11-17 | セキュリティの強化（#592〜#599・#602） |
| v0.28.0 | 2026-11-18 | MCPの2026-07-28版に、新旧の両方を受ける形で対応する（#570）、`mcp-rs check`で`[database]`・`[[plugins]]`も確かめる（#589） |
| v0.29.0 | 2026-11-19 | 導入を簡単にする（続き）と使いやすさ: `init`でデータベース（#590）、`database-keyring`（#628）、ScoopとHomebrew（#578）、`uninstall`（#636）、新しい版の知らせ（#635）、投稿の検索（#637）、予約投稿（#625）、使われていないモジュールの整理（#638）、小さな強化（#598） |

---

## 🚀 リリース計画

マイルストーンの番号は、リリースの版（クレートの`Cargo.toml`の`version`）と同じにする（2026-11-17から）。次のリリースに入れるものはその版のマイルストーンに、後に行うものはv1.0.0（2027年6月頃）に入れる。1つの版に1つの目的を持たせる。

### 🔜 v0.30.0（[マイルストーン](https://github.com/n-takatsu/mcp-rs/milestone/9)）

- #570 の残り: Copilot（新旧の両方に対応）から、`initialize`に戻らずにつながることを、実機で確かめる（Copilotのクレジットが戻る2026-12-01以降。v0.28.0→v0.29.0→v0.30.0と移した）

ほかに入れるものは、これから決める。

### 📦 v1.0.0（2027年6月頃、[マイルストーン](https://github.com/n-takatsu/mcp-rs/milestone/3)）

MCPのクライアント（Claude Desktop・Claude Code等）から使える機能に絞る（#419で範囲を見直した。SAML・LDAP・MFA・マルチテナント・請求システム・ゼロコピー/分散キャッシュは対象外。SAML・LDAPとの連携は、OAuth 2.1の認可サーバー（IdP）側で行う）。

**導入を簡単にする（続き）**

- #632 Claude Desktopの拡張機能（`.mcpb`）をリリースに付ける（#579の調査の続き）
- #634 wingetで配る（#578の続き。外部の審査と、書き込みの鍵が要る）
- #645 `mcp-rs init`で、WordPressを使わずにデータベースだけを設定する（#590で見送った）
- #631 Dockerイメージを静的リンクにし、土台を`distroless/static`にする（Code scanningの土台の部品の警告を減らす）

**WordPress**（2026-11-19の見直しで加えた）

- #640 1つのmcp-rsで、複数のWordPressサイトを使う
- #639 コメントを管理する（承認・スパム・返信・削除）
- #641 投稿のリビジョンを見て、前の版に戻す
- #642 カスタム投稿タイプと独自のタクソノミー

**MCPの機能**（2026-11-19の見直しで加えた）

- #643 MCPのプロンプト（定型の頼み方）を出す
- #644 リソースのテンプレートで、投稿等を1件ずつ読む

**データベースの対応を広げる**（#526）

データベースのハンドラーは、MySQL・MariaDB・PostgreSQL・SQLite・Redis・MongoDBのエンジンをライブラリとして持っている。用意してあるエンジンを、PostgreSQL（#453）と同じ安全性（既定は読み取りだけ、行数の上限、タイムアウト）で、本体（`[database]`）から使えるようにする。どのエンジンも、リリースのバイナリ・Dockerイメージに含めるかを、それぞれのIssueで決める（MySQL・MariaDBは含めた。約0.7MB増）。

- ✅ #527 MySQL・MariaDB（v0.25.0）、✅ #574 `sql_mode`による検査のすり抜け（v0.26.0）
- #528 SQLite（同梱のSQLiteの脆弱性の扱いを含む。#449 はsqlxの対応待ち）
- #529 Redis、#530 MongoDB
- #646 SQLの実行計画（EXPLAIN）を見るツール
- #609 マイクロセグメンテーション（表ごとの許可）を、`execute_command`にもかける

**プラグイン**

- #615 プラグインのコンテナで、許可した行き先にだけ通信できるようにする（egressの許可リスト）
- #616 プラグインを、ストアのように探して入れ、管理する

**そのほか**

- #617 設定ファイルのエラーで項目の名前を伏せない・STDIOモードでCtrl+Cを受けて終わる
- #449 sqlxが対応したらRUSTSEC-2026-0459（同梱のSQLiteの脆弱性）の無視を外す（sqlxの対応待ち）

---

## 🔗 関連リソース

## 📋 GitHub Issues

- [Milestone v0.30.0](https://github.com/n-takatsu/mcp-rs/milestone/9) - Copilotでの確認（#570）ほか
- [Milestone v1.0.0](https://github.com/n-takatsu/mcp-rs/milestone/3) - v0.30.0より後、v1.0.0までに行うIssue
- 完了: [v0.29.0](https://github.com/n-takatsu/mcp-rs/milestone/8)（導入を簡単にする（続き）と使いやすさ）・[v0.27.0](https://github.com/n-takatsu/mcp-rs/milestone/6)（セキュリティの強化）・[v0.28.0](https://github.com/n-takatsu/mcp-rs/milestone/7)（MCPの新しい版）・[Epic #41](https://github.com/n-takatsu/mcp-rs/issues/41)（HTTPモードの認可・ツールの権限・監査ログ）

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

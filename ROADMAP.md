# 🗺️ mcp-rs プロジェクト ロードマップ

> **最終更新**: 2026年11月17日
> **バージョン**: v1.8
> **ステータス**: アクティブ開発中

## 🎯 プロジェクト概要

mcp-rsは、Rustで書いたModel Context Protocol (MCP) サーバーです。Claude Desktop・Claude Code・VS Code（Copilot）等のAIクライアントから、WordPressのサイトと、データベース（PostgreSQL・MySQL・MariaDB）を安全に使えるようにします。

## 🏆 プロジェクト目標

2026-11-17に、今のmcp-rsに合わせて書き直した。

- **安全が既定**: AIのクライアントに渡す権限を、既定で小さくする。データベースは読み取りだけ・上限付き、HTTPモードは認証が要る、ツールの権限と監査ログ、SQLの検査
- **導入が簡単**: ビルド済みのファイル、インストールスクリプト、`mcp-rs init`、`mcp-rs check`で、設定ファイルを手で書かずに使い始められる
- **MCPのクライアントで使えるものに絞る**: Claude Desktop・Claude Code・VS Code（Copilot）から使える機能を作る（#419で範囲を見直した）
- **確かめてから出す**: CIで、実際のデータベース（PostgreSQL・MySQL・MariaDB）と3つのOSで確かめる。Rustで書く

---

## 📜 これまでの変更

完了した機能・リリースの履歴は、[CHANGELOG.md](CHANGELOG.md)・[RELEASE_NOTES.md](RELEASE_NOTES.md)・[GitHub Releases](https://github.com/n-takatsu/mcp-rs/releases)を参照してください。このロードマップには、これからの計画と、今のマイルストーンで完了した範囲を載せます。

---

## 🚀 リリース計画

## 📦 v1.0.0 (2027年6月頃 - 週2日ペース想定)

**テーマ**: HTTPモードを組織で安全に使えるようにし、導入を簡単にし、用意してあるデータベースを本体から使えるようにする

MCPのクライアント（Claude Desktop・Claude Code等）から使える機能に絞る。2026-11-08に範囲を見直した（#419）。SAML・LDAP・MFA・マルチテナント・請求システム・ゼロコピー/分散キャッシュは対象外とした（SAML・LDAPとの連携は、OAuth 2.1の認可サーバー（IdP）側で行う）。

マイルストーンの番号は、リリースの版（クレートの`Cargo.toml`の`version`）と同じにする（2026-11-17から。それまでの「v1.0.0」は、版とは別の計画の区切りだった）。次のリリースに入れるものは、その版のマイルストーン（v0.27.0〜v0.29.0）に、v0.29.0より後に行うものはv1.0.0（2027年6月頃）に入れる。

2026-11-17に、v0.26.0のリリースとコードの点検の後に並べ直した（セキュリティ → MCPの新しい版 → 導入の続き → データベースの順。1つの版に1つの目的）。

### ✅ 完了: HTTPモードの認可・ツールの権限・監査ログ（Epic #41）

2026-11-08に完了した（#421・#416・#417・#420・#52・#55）。詳細はCHANGELOG.mdを参照。

### ✅ 完了: 導入を簡単にする（v0.25.0・v0.26.0、2026-11-17）

使い始めるのに、ダウンロードと確認・資格情報の登録・TOMLの設定ファイル・クライアントごとの登録（絶対パス）・再起動を手で行っていたのを、2つのコマンド（インストールスクリプト → `mcp-rs init`）にした。

- #564 リリースに、ビルド済みのmcp-rsを付ける（v0.25.0）
- #565 設定を確かめるコマンド`mcp-rs check`（v0.25.0）
- #575 対話式の初期設定のコマンド`mcp-rs init`（v0.26.0）
- #576 1行で入れるインストールスクリプト（v0.26.0）
- #577 READMEの導入手順を短くし、導入ガイドを分ける（v0.26.0）

### 🔜 v0.27.0: セキュリティの強化（[マイルストーン](https://github.com/n-takatsu/mcp-rs/milestone/6)）

2026-11-17のコードの点検で見つかったもの。直し方の多くが、今の設定で動いているものを止める（互換性の無い）変更なので、パッチ版ではなくマイナー版にし、移行の注意を1つのリリースノートにまとめる。ほかの大きな変更（MCPの新しい版）と混ぜず、セキュリティだけの版にする。

- #592 HTTP/2の何も送らない接続が、接続数の上限を埋められる
- #593 同じマシンのリバースプロキシの後ろでは、認証が無くても起動する
- #594 MySQL・MariaDBの`verify-ca`で、`ssl_root_cert`が無いと、どのホストの公開の証明書でも受け入れる
- #595 プラグインのツールの「読み取りだけ」の印を、`@read_only`が既定で信じない
- #596 HTTPモードの小さな強化（ログ・WebSocket・APIキー・状態の表示）。`initialize`の項目は、v0.28.0の#570で直す
- #597 MySQL・MariaDBのSQLの検査を、もう一度点検する（点検が途中で止まった）
- #599 `mcp-rs init`とインストールの小さな強化（控えのパスワード・ファイルの権限・設定の書き方・リリースの出所の証明）。v0.29.0から移した
- #602 `mcp-rs init`で、前の設定ファイルにしか無い設定を知らせ、引き継げるようにする。v0.26.0への移行で、`[[plugins]]`等が抜け落ちたため（2026-11-17に追加。セキュリティではないが、#599と同じところを直すので、この版に入れた）

### 📅 v0.28.0: MCPの2026-07-28版に対応する（[マイルストーン](https://github.com/n-takatsu/mcp-rs/milestone/7)）

MCPの仕様の2026-07-28版で、`initialize`の手続きとセッションが無くなった。今のmcp-rs（2025-11-25版）は、新旧の両方に対応したクライアントとは仕様どおりにつながるが、新しい版だけのクライアントとはつながらない。仕様には、`initialize`をいつまで使えるかの決まりが無く、クライアントがいつ新しい版だけになるかは、こちらで決められない。今のクライアントを壊さないよう、新旧の両方を受ける形にする（#566で調べた）。

- #570 MCPの2026-07-28版（initializeの廃止）に、新旧の両方を受ける形で対応する（STDIO→HTTP→無くなったものの順）
- #589 `mcp-rs check`で、`[database]`と`[[plugins]]`も確かめる（起動に失敗しても、ツールが無いまま動くので、理由が分かりにくい）

### 📅 v0.29.0: 導入を簡単にする（続き）（[マイルストーン](https://github.com/n-takatsu/mcp-rs/milestone/8)）

- #578 パッケージマネージャー（winget・Homebrew・Scoop）で配る
- #579 Claude Desktopの拡張機能（`.mcpb`）で配れるかを調べる（Claude Desktopの画面で設定を入力できる形）
- #590 `mcp-rs init`で、`[database]`（PostgreSQL・MySQL・MariaDB）も聞く（データベースのパスワードを、資格情報ストアから読めるようにするかを含む）
- セキュリティの小さな強化: #598 WordPressとプラグイン（応答の大きさ・ツールの名前）

### 📅 v0.29.0より後: データベースの対応を広げる（2026-11-13に追加、#526）

データベースのハンドラーは、MySQL・MariaDB・PostgreSQL・SQLite・Redis・MongoDBのエンジンをライブラリとして持っている。用意してあるエンジンを、PostgreSQL（#453）と同じ安全性（既定は読み取りだけ、行数の上限、タイムアウト）で、本体（`[database]`）から使えるようにする。

- **Phase 4: 土台**（✅ 2026-11-13に完了）
  - #513 サーバーが要る結合テスト（PostgreSQL・Redis）をCIのサービスのコンテナで実行する
- **Phase 5: SQLのデータベース**
  - ✅ #527 MySQL・MariaDB（v0.25.0。#572・#573）
  - ✅ #574 `sql_mode`による、SQLの検査のすり抜け（v0.26.0。#585）
  - #528 SQLite（同梱のSQLiteの脆弱性の扱いを含む。#449 はsqlxの対応待ち）
- **Phase 6: SQLではないデータベース**
  - #529 Redis
  - #530 MongoDB

どのエンジンも、リリースのバイナリ・Dockerイメージに含めるかを、それぞれのIssueで決める（バイナリの大きさと、`cargo audit`の対象が増える）。MySQL・MariaDBは含めた（約0.7MB増）。

### ✅ 2026-11-15〜17に完了（v0.25.0・v0.26.0）

- #521 VS Code（Copilot）からmcp-rsを使う手順（#563）
- #564・#565・#575・#576・#577 導入を簡単にする（上）
- #566 MCPの新しい版（`server/discover`）の調査（#570を起票）
- #527 `[database]`でMySQL・MariaDBを使えるようにする（#572・#573）
- #574 MySQL・MariaDBの`sql_mode`による、SQLの検査のすり抜け（#585）

### 📝 そのほか

- #449 sqlxが対応したらRUSTSEC-2026-0459（同梱のSQLiteの脆弱性）の無視を外す（sqlxの対応待ち）

---

## 🔗 関連リソース

## 📋 GitHub Issues

- [Milestone v0.27.0](https://github.com/n-takatsu/mcp-rs/milestone/6) - セキュリティの強化
- [Milestone v0.28.0](https://github.com/n-takatsu/mcp-rs/milestone/7) - MCPの新しい版
- [Milestone v0.29.0](https://github.com/n-takatsu/mcp-rs/milestone/8) - 導入を簡単にする（続き）
- [Milestone v1.0.0](https://github.com/n-takatsu/mcp-rs/milestone/3) - v0.29.0より後、v1.0.0までに行うIssue
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

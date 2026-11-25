# 🗺️ mcp-rs プロジェクト ロードマップ

> **最終更新**: 2026年11月21日
> **バージョン**: v1.16
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

リリースが予定より早く進んでいるので、2026-11-19の見直しで、1つの版に入れる項目を増やし、v1.0.0までをv0.30.0・v0.31.0の2つの版にまとめた。v1.0.0の目安（2027年6月頃）は変えず、早く進んだ分は、安定化と実際に使う期間に充てる。

2026-11-24の見直しで、v0.30.0は、過去の版のレビュー（CodeRabbit）の指摘の修正で大きくなったので、mcp-rs本体に当たる修正と使いやすさに絞った「本体の強化」の版にした。v0.31.0はデータベースだけの版にし、プラグインと配布はv0.32.0に分けた。

2026-11-26の見直しで、WordPressの道具を広げる項目（#783・#786・#788・#789）のために、v0.33.0「WordPressの強化」を足した。#789はv0.32.0のプラグインの仕組み（#787）の上に作るので、v0.32.0の後に置く。v0.30.0のレビューで後回しにした指摘（#785）は、データを壊しうる指摘を含むので、データベースの版のv0.31.0で直す（データベースだけの方針の例外）。

MCPのクライアント（Claude Desktop・Claude Code等）から使える機能に絞る（#419で範囲を見直した。SAML・LDAP・MFA・マルチテナント・請求システム・ゼロコピー/分散キャッシュは対象外。SAML・LDAPとの連携は、OAuth 2.1の認可サーバー（IdP）側で行う）。

### 🔜 v0.30.0: 本体の強化（WordPress・MCP・導入）（[マイルストーン](https://github.com/n-takatsu/mcp-rs/milestone/9)）

2026-11-26にリリースした。#668（公式のMCP Registryへの登録）が残っているので、マイルストーンは開けたままにする。

**WordPress**

- #640 1つのmcp-rsで、複数のWordPressサイトを使う
- #639 コメントを管理する（承認・スパム・返信・削除）
- #641 投稿のリビジョンを見て、前の版に戻す
- #642 カスタム投稿タイプと独自のタクソノミー
- #680 `create_post_with_embeds`で、YouTube ShortsのURLも動画として埋め込む（#679のレビューで指摘された）

**MCPの機能**

- #643 MCPのプロンプト（定型の頼み方）を出す
- #644 リソースのテンプレートで、投稿等を1件ずつ読む

**導入を簡単にする（続き）**

- #632 Claude Desktopの拡張機能（`.mcpb`）をリリースに付ける（#579の調査の続き）
- #668 公式のMCP Registryに、`.mcpb`で登録する（#632の続き。v0.30.0のリリースの後に登録する）
- #645 `mcp-rs init`で、WordPressを使わずにデータベースだけを設定する（#590で見送った）
- #617 設定ファイルのエラーで項目の名前を伏せない・STDIOモードでCtrl+Cを受けて終わる
- #667 設定ファイルが無くても、STDIOモードで`mcp_rs_status`だけで起動する（Glamaへの登録でつまずいた）
- #674 Windowsで、設定ファイル・控え・一時ファイルを本人だけが読めるACLで書く（#671のレビューで指摘された）
- #676 ツールの説明と引数の説明を見直して、AIのクライアントが選びやすくする（Glamaの評価がCで、その主な理由がツールの説明の質のため。#668より前に行う）
- #631 Dockerイメージを静的リンクにし、土台を`distroless/static`にする（Code scanningの土台の部品の警告を減らす。警告が増え続け、対応が要る警告が埋もれるので、v0.31.0から前倒しした）

**過去の版のレビュー（CodeRabbit）の指摘**

v0.15.0〜v0.21.0の変更と、v0.16.0以前からある`src`を、今のdevelopの中身でレビューし、本体に当たる指摘を直した。

- ✅ #744・#746・#750・#755 各版の変更のレビューの指摘
- ✅ #759 v0.16.0以前からある`src`の、本体に当たる35件（WordPress・HTTP・MCPのサーバー・IDS・ログ・終了・MySQL）
- #760 本体が使わないライブラリのコードを非公開にし、公開のまま残すAPIの指摘を直す
- #758 PostgreSQLのエンジン（ライブラリ）の`health_check`・`get_version`を接続先に問い合わせる。MongoDB・Redisは`[database]`で使えないことを書く

### 📅 v0.31.0: データベース（[マイルストーン](https://github.com/n-takatsu/mcp-rs/milestone/10)）

**データベースの対応を広げる**（#526）

データベースのハンドラーは、MySQL・MariaDB・PostgreSQL・SQLite・Redis・MongoDBのエンジンをライブラリとして持っている。用意してあるエンジンを、PostgreSQL（#453）と同じ安全性（既定は読み取りだけ、行数の上限、タイムアウト）で、本体（`[database]`）から使えるようにする。どのエンジンも、リリースのバイナリ・Dockerイメージに含めるかを、それぞれのIssueで決める（MySQL・MariaDBは含めた。約0.7MB増）。

- ✅ #527 MySQL・MariaDB（v0.25.0）、✅ #574 `sql_mode`による検査のすり抜け（v0.26.0）
- #528 SQLite（同梱のSQLiteの脆弱性の扱いを含む。#449 はsqlxの対応待ち）
- #529 Redis、#530 MongoDB
- #646 SQLの実行計画（EXPLAIN）を見るツール
- #609 マイクロセグメンテーション（表ごとの許可）を、`execute_command`にもかける
- #768 データベースのライブラリのコード（列の暗号化・ゼロトラスト・MySQL）のレビューの指摘

**v0.30.0のレビューで後回しにした指摘**

- #785 `/wp/v2`の外のタクソノミーのIDを確かめる・`init`で新しい設定ファイルに置き換えるときの、質問中の編集を確かめる

### 📅 v0.32.0: プラグインと配布（[マイルストーン](https://github.com/n-takatsu/mcp-rs/milestone/11)）

**プラグイン**

- #615 プラグインのコンテナで、許可した行き先にだけ通信できるようにする（egressの許可リスト）
- #616 プラグインを、ストアのように探して入れ、管理する
- #766 STDIOで取り消したプラグインの呼び出しを、プラグインにも取り消しとして伝える
- #775 プラグインの起動の途中でも、止める合図で後片付けして終わる
- #787 プラグインに、本体に登録したWordPressのサイトを使わせる（資格情報の受け渡しと通信先の制限。#789の土台）

**配布**

- #634 wingetで配る（#578の続き。外部の審査と、書き込みの鍵が要る）
- #669 Claude Desktopの拡張機能の公式ディレクトリに申請する（#632の続き。実際のClaude Desktopでの確認と、Anthropicの審査が要る）

### 📅 v0.33.0: WordPressの強化（[マイルストーン](https://github.com/n-takatsu/mcp-rs/milestone/12)）

WordPress本体の機能は本体の道具に足し、プラグイン（WooCommerce等）向けの機能は、v0.32.0のプラグインの仕組み（#787）に載せる。

- #786 WordPress本体のまだ使っていない機能を道具に足す（メニュー・再利用ブロック・サイト内検索・プラグインの一覧）
- #783 SEOの改善に使う道具（SEOプラグインの値・alt・内部リンク・古い記事）
- #788 許可した名前空間だけを通す、汎用のRESTツール
- #789 WordPressのメジャーなプラグインと連携するプラグイン（Redirection・WooCommerce・ACF・多言語）

### 📦 v1.0.0（2027年6月頃、[マイルストーン](https://github.com/n-takatsu/mcp-rs/milestone/3)）

v0.33.0までで機能をそろえ、v1.0.0までは安定化と、実際に使って見つかった問題の修正に充てる。

- #449 sqlxが対応したらRUSTSEC-2026-0459（同梱のSQLiteの脆弱性）の無視を外す（sqlxの対応待ち）
- #658 設定ファイルの版と、`mcp-rs config migrate`（互換性の無い変更のときに設定ファイルを更新する）
- #662 プラグインのMCPのプロンプトを中継する

---

## 🔗 関連リソース

## 📋 GitHub Issues

- [Milestone v0.30.0](https://github.com/n-takatsu/mcp-rs/milestone/9) - 本体の強化（WordPress・MCP・導入）
- [Milestone v0.31.0](https://github.com/n-takatsu/mcp-rs/milestone/10) - データベース
- [Milestone v0.32.0](https://github.com/n-takatsu/mcp-rs/milestone/11) - プラグインと配布
- [Milestone v0.33.0](https://github.com/n-takatsu/mcp-rs/milestone/12) - WordPressの強化
- [Milestone v1.0.0](https://github.com/n-takatsu/mcp-rs/milestone/3) - 安定化（v0.33.0より後）
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

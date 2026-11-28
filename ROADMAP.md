# 🗺️ mcp-rs プロジェクト ロードマップ

> **最終更新**: 2026年11月28日
> **バージョン**: v1.20
> **ステータス**: アクティブ開発中

## 🎯 プロジェクト概要

mcp-rsは、Rustで書いたModel Context Protocol (MCP) サーバーです。Claude Desktop・Claude Code・VS Code（Copilot）等のAIクライアントから、WordPressのサイトと、データベース（PostgreSQL・MySQL・MariaDB・SQLite・Redis・MongoDB）を安全に使えるようにします。

## 🏆 プロジェクト目標

2026-11-17に、今のmcp-rsに合わせて書き直した。

- **安全が既定**: AIのクライアントに渡す権限を、既定で小さくする。データベースは読み取りだけ・上限付き、HTTPモードは認証が要る、ツールの権限と監査ログ、SQLの検査
- **導入が簡単**: ビルド済みのファイル、インストールスクリプト、パッケージマネージャー、`mcp-rs init`、`mcp-rs check`で、設定ファイルを手で書かずに使い始められる
- **MCPのクライアントで使えるものに絞る**: Claude Desktop・Claude Code・VS Code（Copilot）から使える機能を作る（#419で範囲を見直した）
- **確かめてから出す**: CIで、実際のデータベース（PostgreSQL・MySQL・MariaDB・Redis・MongoDB）と3つのOSで確かめる。Rustで書く

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
| v0.30.0 | 2026-11-26 | 本体の強化（WordPress・MCP・導入）: 複数のWordPressのサイト（#640・#657）、コメント（#639）、リビジョン（#641）、カスタム投稿タイプ（#642）、YouTube Shorts（#680）、MCPのプロンプト（#643）、リソースのテンプレート（#644）、Claude Desktopの拡張機能（#632）、`init`でデータベースだけ（#645）、設定ファイルが無くても起動（#667）、ツールの説明（#676）、WindowsのACL（#674）、Dockerイメージの静的リンク（#631）、リモートのデータベースで`ssl_mode`を必須に（#722）、過去の版のレビューの指摘（#684・#692・#697・#705・#710・#717・#724・#744・#746・#750・#755・#758・#759・#760）。リリースの後に、公式のMCP Registryに登録（#668） |
| v0.31.0 | 2026-11-28 | データベース: SQLite（#528）、Redis（#529）、MongoDB（#530）、`explain_query`（#646）、マイクロセグメンテーションを`execute_command`にも（#609）、データベースのライブラリのコードのレビューの指摘（#768。列の暗号化は外した）、v0.30.0のレビューで後回しにした指摘（#785）、v0.30.0からの変更のレビューの指摘（#806〜#810）、`upload_media`の許可フォルダの案内（#819） |

---

## 🚀 リリース計画

マイルストーンの番号は、リリースの版（クレートの`Cargo.toml`の`version`）と同じにする（2026-11-17から）。次のリリースに入れるものはその版のマイルストーンに、後に行うものはv1.0.0（2027年6月頃）に入れる。1つの版に1つの目的を持たせる。

リリースが予定より早く進んでいるので、2026-11-19の見直しで、1つの版に入れる項目を増やし、v1.0.0までをv0.30.0・v0.31.0の2つの版にまとめた。v1.0.0の目安（2027年6月頃）は変えず、早く進んだ分は、安定化と実際に使う期間に充てる。

2026-11-24の見直しで、v0.30.0は、過去の版のレビュー（CodeRabbit）の指摘の修正で大きくなったので、mcp-rs本体に当たる修正と使いやすさに絞った「本体の強化」の版にした。v0.31.0はデータベースだけの版にし、プラグインと配布はv0.32.0に分けた。

2026-11-26の見直しで、WordPressの道具を広げる項目（#783・#786・#788・#789）のために、v0.33.0「WordPressの強化」を足した。#789はv0.32.0のプラグインの仕組み（#787）の上に作るので、v0.32.0の後に置く。v0.30.0のレビューで後回しにした指摘（#785）は、データを壊しうる指摘を含むので、データベースの版のv0.31.0で直す（データベースだけの方針の例外）。記事を作るツールの重なりを1つにまとめる項目（#795）もv0.33.0に入れ、WordPressの道具を足す前に行う。

2026-11-27の見直しで、v0.31.0の予定の項目がすべて終わったので、v0.30.0からの変更をまとめてレビューし（#806）、見つかった指摘（#807〜#810）をv0.31.0に入れた。まだ出していないv0.31.0のコードへの修正なので、v0.31.0を出す前に直す。外部の審査や鍵を待つ配布の項目（#634・#669）は、v0.32.0を待たせないよう、v1.0.0に移した。v0.32.0はプラグインだけの版になる。v0.33.0には、WordPressの道具の候補（#811〜#814）を足した。

2026-11-28に、`upload_media`で許可フォルダの外を指したときのエラーに、使える許可フォルダを案内する項目（#819）を、v0.31.0に入れた（データベースだけの方針の例外）。案内が無いと、AIはファイルをbase64で送り直すしかなく、画像1枚に数分かかり、大きなファイルは送れない。

MCPのクライアント（Claude Desktop・Claude Code等）から使える機能に絞る（#419で範囲を見直した。SAML・LDAP・MFA・マルチテナント・請求システム・ゼロコピー/分散キャッシュは対象外。SAML・LDAPとの連携は、OAuth 2.1の認可サーバー（IdP）側で行う）。

### 🔜 v0.32.0: プラグイン（[マイルストーン](https://github.com/n-takatsu/mcp-rs/milestone/11)）

- #615 プラグインのコンテナで、許可した行き先にだけ通信できるようにする（egressの許可リスト）
- #616 プラグインを、ストアのように探して入れ、管理する
- #766 STDIOで取り消したプラグインの呼び出しを、プラグインにも取り消しとして伝える
- #775 プラグインの起動の途中でも、止める合図で後片付けして終わる
- #787 プラグインに、本体に登録したWordPressのサイトを使わせる（資格情報の受け渡しと通信先の制限。#789の土台）

配布の項目（#634・#669）は、外部の審査や鍵を待つので、v1.0.0に移した（2026-11-27）。

### 📅 v0.33.0: WordPressの強化（[マイルストーン](https://github.com/n-takatsu/mcp-rs/milestone/12)）

WordPress本体の機能は本体の道具に足し、プラグイン（WooCommerce等）向けの機能は、v0.32.0のプラグインの仕組み（#787）に載せる。道具を足す前に、重なっている記事の作成・変更のツールをまとめる（#795）。

- #795 記事を作るツールを1つにまとめ、記事を変えるツールの重なりを見直す（TDQSの見分けやすさ。破壊的変更）
- #811 記事の2つの版の差分を返すツール（`compare_revisions`）
- #812 記事をブロック単位で読み、1つだけ差し替えるツール（#795の後）
- #786 WordPress本体のまだ使っていない機能を道具に足す（メニュー・再利用ブロック・サイト内検索・プラグインの一覧）
- #783 SEOの改善に使う道具（SEOプラグインの値・alt・内部リンク・古い記事）
- #788 許可した名前空間だけを通す、汎用のRESTツール
- #789 WordPressのメジャーなプラグインと連携するプラグイン（Redirection・WooCommerce・ACF・多言語）
- #813 登録したWordPressのサイトの間で、記事を写すツール（資格情報の扱いは#787と同じ考え方）
- #814 運用に使う小さな道具（コメントの一括処理・サイトヘルス・使われていないメディア・投稿者・Markdownの書き出し）

### 📦 v1.0.0（2027年6月頃、[マイルストーン](https://github.com/n-takatsu/mcp-rs/milestone/3)）

v0.33.0までで機能をそろえ、v1.0.0までは安定化と、実際に使って見つかった問題の修正に充てる。

- #449 sqlxが対応したらRUSTSEC-2026-0459（同梱のSQLiteの脆弱性）の無視を外す（sqlxの対応待ち）
- #658 設定ファイルの版と、`mcp-rs config migrate`（互換性の無い変更のときに設定ファイルを更新する）
- #662 プラグインのMCPのプロンプトを中継する
- #634 wingetで配る（#578の続き。外部の審査と、書き込みの鍵が要る。v0.32.0から移した）
- #669 Claude Desktopの拡張機能の公式ディレクトリに申請する（#632の続き。実際のClaude Desktopでの確認と、Anthropicの審査が要る。v0.32.0から移した）

---

## 🔗 関連リソース

## 📋 GitHub Issues

- [Milestone v0.32.0](https://github.com/n-takatsu/mcp-rs/milestone/11) - プラグイン
- [Milestone v0.33.0](https://github.com/n-takatsu/mcp-rs/milestone/12) - WordPressの強化
- [Milestone v1.0.0](https://github.com/n-takatsu/mcp-rs/milestone/3) - 安定化（v0.33.0より後）
- 完了: [v0.31.0](https://github.com/n-takatsu/mcp-rs/milestone/10)（データベース）・[v0.30.0](https://github.com/n-takatsu/mcp-rs/milestone/9)（本体の強化（WordPress・MCP・導入））・[v0.29.0](https://github.com/n-takatsu/mcp-rs/milestone/8)（導入を簡単にする（続き）と使いやすさ）・[v0.27.0](https://github.com/n-takatsu/mcp-rs/milestone/6)（セキュリティの強化）・[v0.28.0](https://github.com/n-takatsu/mcp-rs/milestone/7)（MCPの新しい版）・[Epic #41](https://github.com/n-takatsu/mcp-rs/issues/41)（HTTPモードの認可・ツールの権限・監査ログ）

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

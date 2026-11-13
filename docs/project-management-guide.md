# mcp-rs プロジェクト管理ガイド

## 📋 目次

1. [概要](#概要)
2. [Issue・マイルストーン・Projectsの使い分け](#issueマイルストーンprojectsの使い分け)
3. [作業の流れ](#作業の流れ)
4. [Issueテンプレート](#issueテンプレート)
5. [ラベリングシステム](#ラベリングシステム)
6. [ワークフローとオートメーション](#ワークフローとオートメーション)

---

## 概要

mcp-rsの作業は、GitHubのIssueで管理する。1つのIssueを1つのPRで既定のブランチ`develop`に入れ、リリースは`develop`から`main`へのPRで行う。

---

## Issue・マイルストーン・Projectsの使い分け

### 📝 Issue - 個々の作業

機能の追加、バグの修正、文書の更新など、PR 1つで終わる単位で立てる。本文には、背景、やること、完了の条件を書く。PR 1つで終わらない大きな機能は、`epic`ラベルを付けたIssueを立て、個々の作業のIssueからリンクする。

### 🏁 マイルストーン - リリース

リリースのバージョンごとにIssueをまとめる。今開いているのは`v1.0.0`（期日 2027-06-30）。`v0.2.0-beta`・`v0.3.0`は完了してクローズ済み。マイルストーンの進捗は、週次のROADMAPレポート（`roadmap-health.yml`）に出る。

### 🎯 GitHub Projects

`mcp-rs ROADMAP Management`と`mcp-rs Core Development`がある。Issueは自動では追加されないので、使うときは手で追加する。

---

## 作業の流れ

1. **Issueを立てる**: テンプレート（下記）を使う。種類のラベルを付け、優先度を判断できれば付ける（付けなければ自動で`priority: medium`になる）。リリースが決まっていれば、マイルストーンを選ぶ
2. **ブランチを作る**: `develop`から作る（例: `feat/123-short-name`・`fix/…`・`docs/…`・`ci/…`）
3. **PRを出す**: `develop`へ出す。本文に`Closes #123`を書くと、マージでIssueが閉じる
4. **レビューとマージ**: CIが通り、レビューの指摘に対応してからマージする。マージ後はブランチを削除する

---

## Issueテンプレート

`.github/ISSUE_TEMPLATE/`にある。どれも種類のラベルを1つ付ける。

| テンプレート | 使うとき | 付くラベル |
|---|---|---|
| `bug_report.yml` | 既存の機能が、期待や文書のとおりに動かない | `bug` |
| `feature_request.yml` | 新しい機能や改善の提案 | `enhancement` |
| `task.yml` | Epicの下の作業や、個別の作業 | `enhancement`（文書だけなら`documentation`に付け替える） |

優先度とマイルストーンは、テンプレートでは選ばない。Issueのラベルとマイルストーンの欄で付ける。

---

## ラベリングシステム

### Issueのラベルの基準

Issueには、次の種類と優先度を1つずつ付ける（Issueの棚卸しの基準）。付けるのは人で、自動の確かめ（下記）は、種類の付け忘れと決まりの誤りに気づかせる。優先度は、付け忘れると自動で`priority: medium`になる（「未判断」と「mediumと判断した」は区別しない）。

| 分類 | ラベル | 付けるもの |
|------|--------|------------|
| 種類 | `bug` | 既存の機能が、期待や文書のとおりに動かない |
| 種類 | `security` | 脆弱性、認証・認可、秘密の値の扱い、依存関係の脆弱性など、安全に関わる |
| 種類 | `documentation` | 文書（README・docs・コメント等）だけを変える |
| 種類 | `enhancement` | 新しい機能、改善、整理（リファクタリング・不要なコードの削除）、CI・開発環境の整備 |
| 優先度 | `priority: high` | リリースを止める、利用者のデータや安全に直ちに影響する、CIが壊れて開発が止まっている |
| 優先度 | `priority: medium` | 次のリリースまでに対応したい。利用者に見える問題や、放置すると問題が大きくなるもの |
| 優先度 | `priority: low` | 急がない改善・整理、ほかの対応や外部の変化を待っているもの |

- `security`は、`issue-labeler.yml`がコンポーネントとしても付ける。ほかの種類（例: `bug`）と一緒に付いていてもよく、ほかの種類が無いときは`security`を種類とみなす
- ほかに付けてよいのは、コンポーネント（`database`・`websocket`・`docker`・`kubernetes`・`infrastructure`。`issue-labeler.yml`がタイトルから付ける、#517）、`epic`（`roadmap-health.yml`がマイルストーンの集計に使う）、自動で作られるIssueの専用ラベル（`automated`・`cargo-audit-scheduled`・`roadmap-health`・`weekly-report`）だけ
- 自動で作られるIssue（`automated`・`cargo-audit-scheduled`が付くもの）は、確かめの対象外。種類・優先度は、作るワークフローが付けることがある（例: 週次ロードマップは`documentation`・`priority: low`）
- 優先度は3段階で、`priority: high`が最上位（`critical`・`P0`等は使わない）

### 以前のラベル

以前は、優先度を`priority: critical`・`P0-Critical`・`priority-p0-critical`・`priority-critical`・`priority-high`・`P1`等で表していた。#519で、これらが付いていたIssue（すべてクローズ済み）を`priority: high`に、`component: security`を`security`に付け替えて、古いラベルを削除した。

ほかに、GitHubの既定のラベル（`duplicate`・`wontfix`・`question`等）や、以前に付けたトピックのラベル（`authentication`・`tls`・`redis`・`performance`等）が残っている。過去のIssueに付いているので削除はしていないが、新しいIssueには付けない（付けると週次の確かめで「決まりにないラベル」になる）。

### ラベルの自動の確かめ

`.github/workflows/issue-labels.yml`が、`scripts/issue_labels.py`を呼ぶ（#523）。AIは使わず、中身を読まずに決まることだけを自動で行う。

- **Issueが開かれたとき**: 優先度が無ければ`priority: medium`を付ける。種類は付けない（中身を読まないと決まらないため）。決まりにないラベル（古い優先度のラベル等）があるIssueには付けない（`P1`等の上に足すと、古いラベルを外したときに優先度が気づかれずに下がるため）
- **週1回（月曜 10:00 JST）と手動実行**: 開いているIssueに上の既定を付けてから、決まりに沿っていないIssue（種類が1つでない、優先度が1つでない、決まりにないラベルがある）を一覧にする。1件でもあればジョブを失敗にするので、失敗の通知で気づける。一覧はジョブのまとめにも出る。優先度は既定が付くので、一覧に「優先度が0個」が出るのは、ラベルを付けられなかったときと、決まりにないラベルがあるときだけ
- **決まり**: 付けるのは足りない既定だけで、既にあるラベルは付け替えない・外さない。自動で作られるIssueは確かめない

手元から実行する（`gh`でログインしていること）:

```bash
# 開いているIssueを確かめる（ラベルは変えない）
python scripts/issue_labels.py

# 優先度の既定を付けてから確かめる
python scripts/issue_labels.py --fix
```

テスト: `python -m unittest discover -s scripts -p "test_*.py"`（GitHubは使わない）

---

## ワークフローとオートメーション

### 1. Issue Labeler（コンポーネントのラベル）

**ワークフロー**: `.github/workflows/issue-labeler.yml`

- Issueを立てたとき（`opened`）に、タイトルの単語からコンポーネントのラベルを付ける
  - `security`・`RUSTSEC`・セキュリティ・脆弱性 → `security`
  - `database`・`PostgreSQL`・`MySQL`・`Redis`等・データベース → `database`
  - `WebSocket` → `websocket`
  - `Docker`・`Dockerfile` → `docker`・`infrastructure`
  - `Kubernetes`・`k8s`・`Helm` → `kubernetes`・`infrastructure`
- 本文は調べない（コードやファイル名に反応するため）。部分一致ではなく単語の境界で調べる
- 種類と優先度は付けない（#517）

### 2. Issue Labels（優先度の既定と、週次の確かめ）

**ワークフロー**: `.github/workflows/issue-labels.yml`。内容は上の「ラベルの自動の確かめ」。

### 3. ROADMAP Health Check（週次レポート）

**ワークフロー**: `.github/workflows/roadmap-health.yml`

- 毎週月曜 09:00 JSTと手動実行
- 開いているマイルストーンの進捗と、長く開いているIssueを報告するIssueを作る（ROADMAP.mdは書き換えない）

```markdown
Title: 📊 Weekly ROADMAP Health Check - 2026-11-16
Labels: roadmap-health, automated, weekly-report, documentation, priority: low
```

### 4. Scheduled Security Audit

**ワークフロー**: `.github/workflows/scheduled-security-audit.yml`

- 毎日 12:00 JST（03:00 UTC）に`cargo audit`を実行し、脆弱性が見つかったら`cargo-audit-scheduled`・`security`の付いたIssueを作る

---

**最終更新**: 2026年11月13日（#519）
